//! RIPR-SPEC-0249 rule 1: `toBeInstanceOf(C)` pins which branch of a
//! changed predicate ran when the owner returns a different class on each
//! side.
//!
//! `if (status === 404) return new NotFound(); return new Forbidden();` with
//! `expect(errorFor(404)).toBeInstanceOf(NotFound)` fails as soon as the
//! predicate sends 404 to the other return, because a `Forbidden` is not a
//! `NotFound`. The matcher alone is not an exact discriminator (it passes
//! for any instance of `C` or of a subclass), so it reads as one only for a
//! predicate change, only on the whole result of an owner call, and only
//! when the changed `if`'s two branches return `new` instances of two
//! classes the owner's module keeps apart.

use super::*;

/// Classes the language provides. None of them can extend a class the
/// owner's module declares.
const BUILTIN_CLASSES: [&str; 17] = [
    "Error",
    "TypeError",
    "RangeError",
    "SyntaxError",
    "ReferenceError",
    "EvalError",
    "URIError",
    "AggregateError",
    "Object",
    "Array",
    "Map",
    "Set",
    "WeakMap",
    "WeakSet",
    "Date",
    "RegExp",
    "Promise",
];

/// The value pins that read `exact_value` / strong for one change, by test
/// file, assertion line and observed expression: `toBeInstanceOf` on a
/// predicate (RIPR-SPEC-0249) and `toMatchObject` on a returned field
/// (RIPR-SPEC-0250).
#[derive(Debug, Default)]
pub(crate) struct PinFacts {
    /// Test file, assertion line, observed expression, matcher and expected
    /// value of each pin. The expected value keeps a second assertion with
    /// the same matcher on the same line and subject from sharing the pin.
    credited: Vec<(PathBuf, usize, String, String, Option<String>)>,
}

impl PinFacts {
    /// `true` when `assertion` in the test at `file` is a credited pin.
    pub(crate) fn credits(&self, file: &Path, assertion: &TypeScriptAssertion) -> bool {
        self.credited
            .iter()
            .any(|(path, line, observed, matcher, expected)| {
                path == file
                    && *line == assertion.line
                    && *matcher == assertion.matcher
                    && assertion.observed_expression.as_deref() == Some(observed.as_str())
                    && assertion.expected_value_or_variant == *expected
            })
    }

    /// Record `assertion` in the test at `file` as a credited pin.
    pub(crate) fn credit(&mut self, file: &Path, assertion: &TypeScriptAssertion) {
        if let Some(observed) = assertion.observed_expression.clone() {
            self.credited.push((
                file.to_path_buf(),
                assertion.line,
                observed,
                assertion.matcher.clone(),
                assertion.expected_value_or_variant.clone(),
            ));
        }
    }

    /// Adds another guard's credited pins (RIPR-SPEC-0250 match-object pins
    /// join the RIPR-SPEC-0249 class pins).
    pub(crate) fn extend(&mut self, other: PinFacts) {
        self.credited.extend(other.credited);
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.credited.is_empty()
    }
}

/// The identifier that starts `text`, when one does.
pub(crate) fn leading_identifier(text: &str) -> Option<&str> {
    let end = text
        .char_indices()
        .find(|(_, c)| !(c.is_alphanumeric() || matches!(c, '_' | '$')))
        .map_or(text.len(), |(index, _)| index);
    let name = &text[..end];
    name.chars()
        .next()
        .is_some_and(|c| c.is_alphabetic() || matches!(c, '_' | '$'))
        .then_some(name)
}

/// A class the module declares.
#[derive(Debug)]
struct ModuleClass {
    name: String,
    /// The `extends` clause as written, empty when there is none.
    parent: String,
    /// Declared outside every block, so module code and imports see it.
    top_level: bool,
    /// Declared as `export class Name`.
    exported: bool,
}

/// The classes a module declares (`class Name`). `None` when a class
/// declaration has no name the scan can read, or a name repeats.
fn module_classes(masked_module: &str) -> Option<Vec<ModuleClass>> {
    let mut classes: Vec<ModuleClass> = Vec::new();
    for at in identifier_offsets(masked_module, "class") {
        let rest = masked_module[at + "class".len()..].trim_start();
        let name = leading_identifier(rest)?;
        if name == "extends" || classes.iter().any(|known| known.name == name) {
            return None;
        }
        let before = &masked_module[..at];
        let top_level = before.matches('{').count() == before.matches('}').count();
        let exported = before
            .trim_end()
            .strip_suffix("export")
            .is_some_and(|head| {
                !head
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_alphanumeric() || matches!(c, '_' | '$' | '.'))
            });
        let header = &rest[name.len()..];
        let header = &header[..header.find('{')?];
        // A generic header (`class A<T> extends B`) is not read.
        if header.trim_start().starts_with('<') {
            return None;
        }
        let parent = header
            .trim_start()
            .strip_prefix("extends")
            .map_or("", str::trim)
            .to_string();
        classes.push(ModuleClass {
            name: name.to_string(),
            parent,
            top_level,
            exported,
        });
    }
    Some(classes)
}

/// `true` when the module uses `name` anywhere a binding could hide: every
/// occurrence must be a `class name` header, an `extends name` clause, a
/// `new name` expression, an `instanceof name` test or a return type
/// (`): name {`). A declaration, parameter, catch binding, import, export
/// clause, destructuring target, assignment or member use (`name.prototype`)
/// fails, as does a type assertion (`x as name`), so the answer errs towards
/// a binding.
fn module_rebinds(masked_module: &str, name: &str) -> bool {
    identifier_offsets(masked_module, name)
        .into_iter()
        .any(|at| {
            let before = masked_module[..at].trim_end();
            let previous = before
                .rsplit(|c: char| !(c.is_alphanumeric() || matches!(c, '_' | '$')))
                .next()
                .unwrap_or("");
            let after = masked_module[at + name.len()..].trim_start();
            let plain = match previous {
                "class" | "extends" => {
                    after.starts_with(['{', '<']) || after.starts_with("extends")
                }
                "new" => after.starts_with('('),
                "instanceof" => after.starts_with([')', '&', '|', ';', '?']) || after.is_empty(),
                "" => {
                    before
                        .strip_suffix(':')
                        .is_some_and(|head| head.trim_end().ends_with(')'))
                        && after.starts_with('{')
                }
                _ => false,
            };
            !plain
        })
}

/// `true` when the module can reach a class, a global or a reflection API
/// where the class-name scan cannot see it, so a `Symbol.hasInstance` hook
/// or a prototype rewrite could be installed without spelling its name:
/// - a decorator, an ambient `declare`, a `static` member or block;
/// - a global object, `eval`, `Function`, `Object`, `Reflect`, `Symbol` or
///   `Proxy`;
/// - a namespace or dynamic import, which can reach the module's own
///   exports;
/// - a `.constructor` or `.prototype` member, a `prototype` name, a
///   `constructor` name other than a class-member header (destructuring
///   `{ constructor: O }` reaches `Object`), or any bracket with something
///   inside (`x[key]`, `x?.[key]`, `{ [key]: v } = o`, `[1, 2]`; an empty
///   `string[]` type or `[]` literal is fine);
/// - a backslash in code, since `\u0046orbidden` is `Forbidden`;
/// - a hook, prototype or constructor name spelled in string text.
fn module_escapes_name_scan(raw_module: &str, masked_module: &str) -> bool {
    // Any bracket with something inside: a computed key can be built from
    // strings in any position (`x[k]`, `x?.[k]`, `{ [k]: v } = o`).
    let computed_access = masked_module
        .match_indices('[')
        .any(|(at, _)| !masked_module[at + 1..].trim_start().starts_with(']'));
    let namespace_import = identifier_offsets(masked_module, "import")
        .into_iter()
        .any(|at| {
            let rest = masked_module[at + "import".len()..].trim_start();
            rest.starts_with('*') || rest.starts_with('(')
        });
    // `constructor` only as a class-member header (`constructor(`): a
    // destructured `{ constructor: O } = {}` reaches `Object` unnamed.
    let constructor_handle = identifier_offsets(masked_module, "constructor")
        .into_iter()
        .any(|at| {
            let before = masked_module[..at].trim_end();
            let after = masked_module[at + "constructor".len()..].trim_start();
            !(before.ends_with(['{', '}', ';']) && after.starts_with('('))
        });
    masked_module.contains(['@', '\\'])
        || constructor_handle
        || computed_access
        || namespace_import
        || [".constructor", ".prototype"]
            .iter()
            .any(|member| masked_module.contains(member))
        || [
            "declare",
            "static",
            "prototype",
            "globalThis",
            "window",
            "global",
            "self",
            "eval",
            "Function",
            "Object",
            "Reflect",
            "Symbol",
            "Proxy",
        ]
        .iter()
        .any(|name| !identifier_offsets(masked_module, name).is_empty())
        || [
            "hasInstance",
            "setPrototypeOf",
            "__proto__",
            "constructor\"",
            "constructor'",
            "constructor`",
        ]
        .iter()
        .any(|text| raw_module.contains(text))
}

/// The module class named `name` when the owner and the module's importers
/// see that declaration: declared at the top level and not rebound.
fn visible_class<'a>(
    name: &str,
    classes: &'a [ModuleClass],
    masked_module: &str,
) -> Option<&'a ModuleClass> {
    classes
        .iter()
        .find(|class| class.name == name)
        .filter(|class| class.top_level && !module_rebinds(masked_module, name))
}

/// The parent a class's `extends` clause names, when it is one plain name:
/// `extends mixin(Base)` or `extends ns.Base` is not read.
fn plain_parent(parent: &str) -> Option<&str> {
    leading_identifier(parent).filter(|next| next.len() == parent.len())
}

/// `true` when `name` is a builtin class the module does not bind itself.
fn unbound_builtin(name: &str, classes: &[ModuleClass], masked_module: &str) -> bool {
    BUILTIN_CLASSES.contains(&name)
        && !classes.iter().any(|class| class.name == name)
        && !module_rebinds(masked_module, name)
}

/// `true` when every ancestor of `asserted` is a visible module class or an
/// unbound builtin, so no imported or unknown parent can hand it a
/// `Symbol.hasInstance` hook the module scan does not see.
fn chain_resolved(asserted: &str, classes: &[ModuleClass], masked_module: &str) -> bool {
    let mut current = asserted;
    for _ in 0..=classes.len() {
        let Some(class) = visible_class(current, classes, masked_module) else {
            return unbound_builtin(current, classes, masked_module);
        };
        if class.parent.is_empty() {
            return true;
        }
        match plain_parent(&class.parent) {
            Some(next) => current = next,
            None => return false,
        }
    }
    false
}

/// The class a return expression constructs when it is exactly
/// `new X(...)`: nothing may follow the argument list (`new X().as()`,
/// `new X() || y`).
fn constructed_class(expression: &str) -> Option<String> {
    let rest = expression.trim().strip_prefix("new")?;
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let rest = rest.trim_start();
    let class = leading_identifier(rest)?;
    ts_is_one_argument_list(rest[class.len()..].trim_start()).then(|| class.to_string())
}

/// The classes the changed `if` (`line_text`, found once in the owner)
/// returns on its true and false branches, each `return new X(...)`, read by
/// the RIPR-SPEC-0027 branch scan. String and comment text is blanked first
/// so a message such as `"missing"` does not stop the scan; a comment that
/// spans lines fails it.
fn branch_classes(owner_source: &str, line_text: &str) -> Option<(String, String)> {
    let blank = |text: &str| ts_mask_literals_and_comments(text).replace(['"', '\'', '`'], " ");
    let masked = blank(owner_source);
    let lines: Vec<&str> = masked.lines().collect();
    if lines.len() != owner_source.lines().count() {
        return None;
    }
    // A `return` inside an arrow or nested function is not the owner's.
    let body = masked_owner_body(owner_source)?;
    if body.contains("=>") || !identifier_offsets(&body, "function").is_empty() {
        return None;
    }
    let target = blank(line_text);
    // The changed line must open the if-body and leave it open: a one-line
    // `if (c) { return a; }` would hand the scan the enclosing block.
    let opens = target.matches('{').count();
    let closes = target.matches('}').count();
    if !target.trim_end().ends_with('{') || opens != closes + 1 {
        return None;
    }
    // Only an `if` hands control to the next statement when it fails; a
    // changed `case`, `for` or `while` line has other successors.
    let head = target.trim_start();
    if !["if (", "if(", "else if (", "else if("]
        .iter()
        .any(|form| head.starts_with(form))
    {
        return None;
    }
    let mut matching = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.trim() == target.trim())
        .map(|(index, _)| index);
    let start = matching.next()?;
    if matching.next().is_some() {
        return None;
    }
    let (true_return, false_return) = branch_return_expressions(&lines, start)?;
    Some((
        constructed_class(&true_return)?,
        constructed_class(&false_return)?,
    ))
}

/// `true` when an instance of `class` can never pass `toBeInstanceOf(asserted)`:
/// its chain reaches a builtin or the top without meeting `asserted`, and
/// every class on the way is one the module declares.
fn never_instance_of(
    class: &str,
    asserted: &str,
    classes: &[ModuleClass],
    masked_module: &str,
) -> bool {
    let mut current = class;
    for _ in 0..=classes.len() {
        if current == asserted {
            return false;
        }
        // A module class shadows a builtin of the same name (`class Map
        // extends NotFound`), so the module is read first.
        let Some(class) = visible_class(current, classes, masked_module) else {
            // Imported, nested, rebound or unknown classes are not read.
            return unbound_builtin(current, classes, masked_module);
        };
        if class.parent.is_empty() {
            return true;
        }
        match plain_parent(&class.parent) {
            Some(next) => current = next,
            None => return false,
        }
    }
    false
}

/// RIPR-SPEC-0249 rule 1's owner-side condition: the module declares
/// `asserted`, the changed `if` returns `new` instances of two different
/// classes on its two branches, one of them `asserted`, and the other is
/// never an `asserted` instance.
pub(crate) fn ts_owner_branches_split_class(
    owner_source: &str,
    module_source: &str,
    line_text: &str,
    asserted: &str,
) -> bool {
    let Some((true_class, false_class)) = branch_classes(owner_source, line_text) else {
        return false;
    };
    let other = if true_class == asserted {
        false_class
    } else if false_class == asserted {
        true_class
    } else {
        return false;
    };
    let module = ts_mask_literals_and_comments(module_source);
    if !module_is_plain(module_source)
        || module_overrides_instanceof(&module)
        || module_escapes_name_scan(module_source, &module)
    {
        return false;
    }
    let Some(classes) = module_classes(&module) else {
        return false;
    };
    // The test imports `asserted` from this module, so it must be the
    // module's own top-level `export class`, with every ancestor resolved.
    visible_class(asserted, &classes, &module).is_some_and(|class| class.exported)
        && chain_resolved(asserted, &classes, &module)
        && other != asserted
        && never_instance_of(&other, asserted, &classes, &module)
}

/// The module text with comments blanked and string contents blanked
/// (quotes kept), or `None` when it has text this plain lexer does not
/// read: a backslash anywhere, a template literal, a `/` in code that does
/// not open a comment (a regular expression or a division), or an
/// unterminated string or comment.
fn plain_module_code(source: &str) -> Option<String> {
    if source.contains(['\\', '`']) {
        return None;
    }
    let mut out = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' | '\'' => {
                out.push(c);
                loop {
                    let next = chars.next()?;
                    if next == c {
                        out.push(c);
                        break;
                    }
                    if next == '\n' {
                        return None;
                    }
                    out.push(' ');
                }
            }
            '/' if chars.peek() == Some(&'/') => {
                out.push(' ');
                for next in chars.by_ref() {
                    if next == '\n' {
                        out.push('\n');
                        break;
                    }
                    out.push(' ');
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                out.push_str("  ");
                let mut star = false;
                loop {
                    let next = chars.next()?;
                    out.push(if next == '\n' { '\n' } else { ' ' });
                    if star && next == '/' {
                        break;
                    }
                    star = next == '*';
                }
            }
            '/' => return None,
            _ => out.push(c),
        }
    }
    Some(out)
}

/// The identifier at the start of `text` after whitespace, and the rest.
fn next_word(text: &str) -> Option<(&str, &str)> {
    let text = text.trim_start();
    let word = leading_identifier(text)?;
    Some((word, &text[word.len()..]))
}

/// The text inside the bracket pair that `text` opens (after whitespace)
/// with `open`, and the rest after its `close`. Nested pairs are counted.
fn bracketed(text: &str, open: char, close: char) -> Option<(&str, &str)> {
    let text = text.trim_start().strip_prefix(open)?;
    let mut depth = 0usize;
    for (index, c) in text.char_indices() {
        if c == open {
            depth += 1;
        } else if c == close {
            if depth == 0 {
                return Some((&text[..index], &text[index + 1..]));
            }
            depth -= 1;
        }
    }
    None
}

/// The parameter names of a plain parameter list (`a: number, b?: T`):
/// no defaults, destructuring, rest or function types.
fn plain_params(params: &str) -> Option<Vec<&str>> {
    if params.contains(['=', '{', '[', '(', '.']) {
        return None;
    }
    let mut names = Vec::new();
    for param in params.split(',') {
        let param = param.trim();
        if param.is_empty() {
            continue;
        }
        let name = leading_identifier(param)?;
        let after = param[name.len()..].trim_start();
        if !(after.is_empty() || after.starts_with([':', '?'])) {
            return None;
        }
        names.push(name);
    }
    Some(names)
}

/// Words a plain function body may use besides its parameters and the
/// class names the module declares or the builtins.
const PLAIN_BODY_WORDS: [&str; 11] = [
    "if",
    "else",
    "return",
    "throw",
    "new",
    "instanceof",
    "typeof",
    "true",
    "false",
    "null",
    "undefined",
];

/// `true` when `body` only tests its parameters and returns or throws new
/// instances: every word is a parameter, a word above or a class name;
/// there is no member access (`.`), bracket, arrow, assignment, local
/// declaration or call other than `new C(...)` and `if (...)`.
fn plain_function_body(body: &str, params: &[&str], classes: &[&str]) -> bool {
    if body.contains(['.', '[', ']', '#', '@', '^', '~']) || body.contains("=>") {
        return false;
    }
    // Comparisons only: an `=` must be part of `==`, `!=`, `<=` or `>=`,
    // never an assignment (`x = y`, `x &&= y`, `x <<= y`, `x++`).
    let bytes = body.as_bytes();
    let assigns = body.contains("<<=")
        || body.contains(">>=")
        || body.contains("++")
        || body.contains("--")
        || bytes.iter().enumerate().any(|(index, &b)| {
            b == b'='
                && !(index > 0 && b"=!<>".contains(&bytes[index - 1]))
                && bytes.get(index + 1) != Some(&b'=')
        });
    if assigns {
        return false;
    }
    let mut previous_word = "";
    let mut rest = body;
    while let Some(c) = rest.chars().next() {
        if c.is_alphabetic() || matches!(c, '_' | '$') {
            let Some(word) = leading_identifier(rest) else {
                return false;
            };
            if !(params.contains(&word)
                || PLAIN_BODY_WORDS.contains(&word)
                || classes.contains(&word)
                || BUILTIN_CLASSES.contains(&word))
            {
                return false;
            }
            previous_word = word;
            rest = &rest[word.len()..];
            continue;
        }
        if c == '(' && params.contains(&previous_word) {
            return false; // a call through a parameter
        }
        if !c.is_whitespace() {
            previous_word = "";
        }
        if !(c.is_alphanumeric() || c.is_whitespace() || "\"'=!<>&|+-*%?:(){},;".contains(c)) {
            return false;
        }
        rest = &rest[c.len_utf8()..];
    }
    true
}

/// `true` when a class body is empty or only a constructor that passes its
/// parameters to `super(...)`.
fn plain_class_body(body: &str) -> bool {
    let body = body.trim();
    if body.is_empty() {
        return true;
    }
    let Some(rest) = body.strip_prefix("constructor") else {
        return false;
    };
    let Some((params, rest)) = bracketed(rest, '(', ')') else {
        return false;
    };
    let Some(params) = plain_params(params) else {
        return false;
    };
    let Some((inner, rest)) = bracketed(rest, '{', '}') else {
        return false;
    };
    if !rest.trim().is_empty() {
        return false;
    }
    let Some(call) = inner.trim().strip_prefix("super") else {
        return false;
    };
    let Some((args, rest)) = bracketed(call, '(', ')') else {
        return false;
    };
    let tail = rest.trim();
    (tail.is_empty() || tail == ";")
        && !args.contains(['(', '{', '[', '.', '='])
        && args.split(',').all(|arg| {
            let arg = arg.trim();
            arg.is_empty()
                || params.contains(&arg)
                || (arg.starts_with(['"', '\''])
                    && arg.len() >= 2
                    && arg.ends_with(arg.as_bytes()[0] as char))
                || arg.chars().all(|c| c.is_ascii_digit())
        })
}

/// `true` when the module is only plain items, so no code in it can reach
/// a class, its prototype or a reflection API:
/// - `import ... from "...";` statements;
/// - `class Name [extends Parent] { }`, empty or with a constructor that
///   only calls `super` with its parameters or literals;
/// - `function name(params)[: Type] { ... }` whose body is plain (see
///   `plain_function_body`);
///
/// each optionally `export`ed. Anything else (a top-level statement, a
/// variable, a type alias, an interface, a decorator) fails.
fn module_is_plain(source: &str) -> bool {
    let Some(code) = plain_module_code(source) else {
        return false;
    };
    // Every other scan reads the shared masker's output. Where the two
    // lexers disagree on what is code (the masker reads a quote after a
    // word as JSX text), text this grammar admits could be hidden from
    // those scans, so the module is not read.
    if code != ts_mask_literals_and_comments(source) {
        return false;
    }
    let mut classes: Vec<&str> = Vec::new();
    let mut functions: Vec<(Vec<&str>, &str)> = Vec::new();
    let mut rest = code.as_str();
    loop {
        if rest.trim().is_empty() {
            break;
        }
        let Some((mut word, mut after)) = next_word(rest) else {
            return false;
        };
        if word == "import" {
            // `import ... from "x";` or `import "x";`: the statement must
            // end at its module string, so it cannot swallow a statement
            // written without a semicolon.
            let Some(end) = after.find(';') else {
                return false;
            };
            let statement = &after[..end];
            let module = statement.trim_end();
            // Before the module string: nothing, or a clause of names,
            // braces, commas, `as` and `*` followed by `from`.
            let ends_at_module = module.ends_with(['"', '\''])
                && module[..module.len() - 1]
                    .rfind(['"', '\''])
                    .is_some_and(|open| {
                        let head = module[..open].trim_end();
                        head.is_empty()
                            || head.strip_suffix("from").is_some_and(|clause| {
                                let clause = clause.trim();
                                !clause.is_empty()
                                    && clause.chars().all(|c| {
                                        c.is_alphanumeric()
                                            || c.is_whitespace()
                                            || matches!(c, '_' | '$' | '{' | '}' | ',' | '*')
                                    })
                            })
                    });
            if statement.contains(['(', ';']) || !ends_at_module {
                return false;
            }
            rest = &after[end + 1..];
            continue;
        }
        if word == "export" {
            let Some(next) = next_word(after) else {
                return false;
            };
            (word, after) = next;
        }
        match word {
            "class" => {
                let Some((name, mut tail)) = next_word(after) else {
                    return false;
                };
                if let Some(("extends", parent_tail)) = next_word(tail) {
                    let Some((_, parent_rest)) = next_word(parent_tail) else {
                        return false;
                    };
                    tail = parent_rest;
                }
                let Some((body, remaining)) = bracketed(tail, '{', '}') else {
                    return false;
                };
                if !plain_class_body(body) {
                    return false;
                }
                classes.push(name);
                rest = remaining;
            }
            "function" => {
                let Some((_, tail)) = next_word(after) else {
                    return false;
                };
                let Some((params, tail)) = bracketed(tail, '(', ')') else {
                    return false;
                };
                let Some(params) = plain_params(params) else {
                    return false;
                };
                let tail = tail.trim_start();
                // A return type runs to the body's `{`.
                let tail = match tail.strip_prefix(':') {
                    Some(annotation) => {
                        let Some(open) = annotation.find('{') else {
                            return false;
                        };
                        // A type name, union, intersection or generic only:
                        // a scan to the next `{` must not cross a statement.
                        if !annotation[..open].chars().all(|c| {
                            c.is_alphanumeric()
                                || matches!(c, ' ' | '\t' | '_' | '$' | '|' | '&' | '<' | '>' | ',')
                        }) {
                            return false;
                        }
                        &annotation[open..]
                    }
                    None => tail,
                };
                let Some((body, remaining)) = bracketed(tail, '{', '}') else {
                    return false;
                };
                functions.push((params, body));
                rest = remaining;
            }
            _ => return false,
        }
    }
    functions
        .iter()
        .all(|(params, body)| plain_function_body(body, params, &classes))
}

/// `true` when the module can make `instanceof` disagree with the `extends`
/// chain: a `Symbol.hasInstance` hook (inherited by subclasses), a
/// prototype rewrite, or a constructor that can return another object.
fn module_overrides_instanceof(masked_module: &str) -> bool {
    // Plain text, not identifier offsets: these are member names
    // (`Symbol.hasInstance`, `Object.setPrototypeOf`).
    if ["hasInstance", "setPrototypeOf", "__proto__"]
        .iter()
        .any(|name| masked_module.contains(name))
    {
        return true;
    }
    identifier_offsets(masked_module, "constructor")
        .into_iter()
        .any(|at| {
            let rest = &masked_module[at..];
            let Some(open) = rest.find('{') else {
                return true;
            };
            let mut depth = 0usize;
            for (index, c) in rest[open..].char_indices() {
                match c {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            let body = &rest[open..open + index];
                            return !identifier_offsets(body, "return").is_empty();
                        }
                    }
                    _ => {}
                }
            }
            true
        })
}

/// `true` when the test's `class` is the owner module's class: imported
/// under its own name from the owner's module, and not rebound in the test
/// or an enclosing scope (`const NotFound = Error`).
fn test_binds_owner_module_class(
    test: &TypeScriptTest,
    class: &str,
    owner: &TypeScriptOwner,
    alias_map: Option<&TsAliasMap>,
    workspace_root: Option<&Path>,
) -> bool {
    let imported = test.imports_in_file.iter().any(|import| {
        !import.namespace
            && import.local == class
            && import.imported.as_deref() == Some(class)
            && import_source_matches_owner(import, &test.file, owner, alias_map, workspace_root)
    });
    imported
        && !local_identifier_declared_in_test_body(&test.body_text, class)
        && !test
            .scope_bindings
            .iter()
            .any(|binding| binding.name == class)
}

/// Collect the credited `toBeInstanceOf` assertions for a predicate change:
/// each on an oracle relation, on the whole result of one owner call
/// (`owner(...)`, `ns.owner(...)`, awaited, or a local bound once to that
/// call), naming the class one branch of the changed `if` returns.
pub(crate) fn ts_instance_class_facts(
    probe_family: &ProbeFamily,
    line_text: &str,
    owner: &TypeScriptOwner,
    candidates: &[TypeScriptRelatedCandidate<'_>],
    alias_map: Option<&TsAliasMap>,
    workspace_root: Option<&Path>,
) -> PinFacts {
    let mut facts = PinFacts::default();
    if !matches!(probe_family, ProbeFamily::Predicate) {
        return facts;
    }
    let pins = |assertion: &TypeScriptAssertion| {
        assertion.matcher == "toBeInstanceOf" && assertion.expected_value_or_variant.is_some()
    };
    if !candidates
        .iter()
        .any(|candidate| candidate.test.assertions.iter().any(pins))
    {
        return facts;
    }
    let Some(owner_source) = owner.source_text.as_deref() else {
        return facts;
    };
    let Some(module_source) = workspace_root
        .and_then(|root| {
            crate::analysis::committed_source::read_source_bytes(root, &owner.file)
                .ok()
                .flatten()
        })
        .and_then(|bytes| String::from_utf8(bytes).ok())
    else {
        return facts;
    };
    let mut owner_side: Vec<(String, bool)> = Vec::new();
    for candidate in candidates
        .iter()
        .filter(|candidate| candidate.relation.uses_oracle())
    {
        if local_identifier_declared_in_test_body(&candidate.test.body_text, &owner.name) {
            continue;
        }
        let receivers = owner_namespace_receivers(candidate.test, owner, alias_map, workspace_root);
        for assertion in candidate.test.assertions.iter().filter(|a| pins(a)) {
            let Some(class) = assertion.expected_value_or_variant.as_deref() else {
                continue;
            };
            let owner_ok = match owner_side.iter().find(|(known, _)| known == class) {
                Some((_, ok)) => *ok,
                None => {
                    let ok = ts_owner_branches_split_class(
                        owner_source,
                        &module_source,
                        line_text,
                        class,
                    );
                    owner_side.push((class.to_string(), ok));
                    ok
                }
            };
            if owner_ok
                && test_binds_owner_module_class(
                    candidate.test,
                    class,
                    owner,
                    alias_map,
                    workspace_root,
                )
                && ts_observes_whole_owner_result(candidate.test, assertion, owner, &receivers)
            {
                facts.credit(&candidate.test.file, assertion);
            }
        }
    }
    facts
}

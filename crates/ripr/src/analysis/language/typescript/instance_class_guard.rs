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

/// The `toBeInstanceOf` assertions that read `exact_value` / strong for one
/// predicate change, by test file and assertion line.
#[derive(Debug, Default)]
pub(crate) struct InstanceClassFacts {
    /// Test file, assertion line and observed expression of each pin.
    credited: Vec<(PathBuf, usize, String)>,
}

impl InstanceClassFacts {
    /// `true` when `assertion` in the test at `file` is a credited
    /// `toBeInstanceOf` pin.
    pub(crate) fn credits(&self, file: &Path, assertion: &TypeScriptAssertion) -> bool {
        assertion.matcher == "toBeInstanceOf"
            && self.credited.iter().any(|(path, line, observed)| {
                path == file
                    && *line == assertion.line
                    && assertion.observed_expression.as_deref() == Some(observed.as_str())
            })
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.credited.is_empty()
    }
}

/// The identifier that starts `text`, when one does.
fn leading_identifier(text: &str) -> Option<&str> {
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
///   `{ constructor: O }` reaches `Object`), or a computed member access
///   (`x[key]`; an empty `string[]` type is fine);
/// - a backslash in code, since `\u0046orbidden` is `Forbidden`;
/// - a hook, prototype or constructor name spelled in string text.
fn module_escapes_name_scan(raw_module: &str, masked_module: &str) -> bool {
    let computed_access = masked_module.match_indices('[').any(|(at, _)| {
        let before = masked_module[..at].trim_end();
        let after = masked_module[at + 1..].trim_start();
        before
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || matches!(c, '_' | '$' | ')' | ']'))
            && !after.starts_with(']')
    });
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
    if module_overrides_instanceof(&module) || module_escapes_name_scan(module_source, &module) {
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
) -> InstanceClassFacts {
    let mut facts = InstanceClassFacts::default();
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
                && let Some(observed) = assertion.observed_expression.clone()
            {
                facts
                    .credited
                    .push((candidate.test.file.clone(), assertion.line, observed));
            }
        }
    }
    facts
}

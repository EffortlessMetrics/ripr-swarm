//! RIPR-SPEC-0247 rule 4 (2026-10-15): a literal `toHaveBeenCalledWith` on
//! a `jest.spyOn` / `vi.spyOn` of an exported module object pins the
//! changed `object.method(...)` call.
//!
//! The spy records every call that reaches `object.method` while it is
//! installed, from anywhere, and `toHaveBeenCalledWith` passes when ANY
//! recorded call matches. So the owner must make no call but the changed
//! one, the test must make no call but the spy, the one owner call and its
//! assertions, and the test file must install the spy once and never run a
//! test twice with it (`each`, `concurrent`, `retryTimes`) or replace the
//! module (`jest.mock`, dynamic `import(`, `require`).

use super::*;
use std::path::{Path, PathBuf};

/// A changed `object.method(...)` call on a module-level object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ModuleObjectCall {
    pub(super) object: String,
    pub(super) method: String,
}

/// A test file whose import `local` binds the owner module's `object`, for
/// a module object call through `method`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SpyTarget {
    pub(super) test_file: PathBuf,
    pub(super) local: String,
    pub(super) method: String,
}

/// Words an owner body may spell besides its parameters, its own locals
/// and the changed call's object: none runs code or reads module state.
const OWNER_WORDS: [&str; 16] = [
    "return",
    "if",
    "else",
    "const",
    "let",
    "var",
    "typeof",
    "void",
    "true",
    "false",
    "null",
    "undefined",
    "as",
    "string",
    "number",
    "boolean",
];

/// Changed-line calls `object.method(...)` whose `object` is an exported
/// top-level `const` of the owner's module that the owner names only in
/// that call, in an owner at the module's top level whose body makes no
/// other call. The spy records calls from any code that runs while the
/// owner does, so the owner's parameter list holds no default (which can
/// run a call or getter) and its body names nothing but its parameters,
/// its own locals, that object and [`OWNER_WORDS`] outside member names:
/// a module binding (`config.on`) can be a getter or a `toString` that
/// reaches the spied method. `owner_source` is the owner's parsed source
/// and `module_source` its whole module.
pub(super) fn module_object_calls(
    line_text: &str,
    owner_source: &str,
    owner_name: &str,
    owner_params: &[String],
    owner_start_line: usize,
    module_source: &str,
) -> Vec<ModuleObjectCall> {
    if !runs_changed_line_once(owner_source, owner_name, line_text) {
        return Vec::new();
    }
    let Some(body) = masked_owner_body(owner_source) else {
        return Vec::new();
    };
    let masked_owner = ts_mask_literals_and_comments(owner_source);
    let header = &masked_owner[..masked_owner.len().saturating_sub(body.len())];
    // A non-ASCII name is no word the allowlist can read. A tagged
    // template needs a callable tag, which no allowed word holds.
    if !body.is_ascii()
        || calls_in(&body).len() != 1
        || has_default(header)
        || !body_words_are_local(&body, owner_params, line_text)
    {
        return Vec::new();
    }
    let masked_module = ts_mask_literals_and_comments(module_source);
    // Module code that imports, or that names the object anywhere but its
    // declaration and the owner's call, can leave a mock with calls on the
    // method (`o.m = jest.fn(); o.m(...)`) before the test's spy installs.
    let imports_or_requires = ["import", "require", "eval"]
        .iter()
        .any(|word| identifier_or_member_offsets(&masked_module, word) != 0);
    if imports_or_requires
        || !line_start(&masked_module, owner_start_line)
            .is_some_and(|at| at_module_top_level(&masked_module, at))
    {
        return Vec::new();
    }
    calls_in(line_text)
        .into_iter()
        .filter(|call| call.depth == 1 && call.root != "this")
        .filter(|call| {
            identifier_offsets(&masked_owner, &call.root).len() == 1
                && identifier_offsets(&masked_module, &call.root).len() == 2
                && exports_top_level_const(&masked_module, &call.root)
        })
        .map(|call| ModuleObjectCall {
            object: call.root,
            method: call.name,
        })
        .collect()
}

/// `true` when the masked owner `header` holds an `=` other than an arrow's
/// `=>` or the `= ` of an arrow owner's own binding (`const f = (`): a
/// parameter default.
fn has_default(header: &str) -> bool {
    let Some(open) = header.find('(') else {
        return false;
    };
    let parameters = &header[open..];
    parameters
        .match_indices('=')
        .any(|(at, _)| !parameters[at + 1..].starts_with('>'))
}

/// `true` when every word of the masked `body` that is not a member name
/// is an owner parameter, a name the body declares with `const`, `let` or
/// `var`, one of [`OWNER_WORDS`], or the root object of a call on
/// `line_text`.
fn body_words_are_local(body: &str, owner_params: &[String], line_text: &str) -> bool {
    let roots: Vec<String> = calls_in(line_text)
        .into_iter()
        .filter(|call| call.depth == 1)
        .map(|call| call.root)
        .collect();
    let locals: Vec<&str> = ["const", "let", "var"]
        .iter()
        .flat_map(|keyword| {
            identifier_offsets(body, keyword)
                .into_iter()
                .map(move |at| at + keyword.len())
        })
        .filter_map(|after| {
            let rest = strip_required_space(&body[after..])?;
            let end = rest
                .find(|c: char| !is_identifier_part(c))
                .unwrap_or(rest.len());
            Some(&rest[..end]).filter(|name| name.starts_with(is_identifier_start))
        })
        .collect();
    let mut at = 0;
    body.split(|c: char| !is_identifier_part(c)).all(|word| {
        let start = at;
        at += word.len() + 1;
        !word.starts_with(is_identifier_start)
            || body[..start].trim_end().ends_with('.')
            || OWNER_WORDS.contains(&word)
            || owner_params.iter().any(|param| param == word)
            || locals.contains(&word)
            || roots.iter().any(|root| root == word)
    })
}

/// The byte offset where 1-based `line` starts.
fn line_start(text: &str, line: usize) -> Option<usize> {
    if line == 0 {
        return None;
    }
    if line == 1 {
        return Some(0);
    }
    text.match_indices('\n').nth(line - 2).map(|(at, _)| at + 1)
}

/// `true` when `at` sits outside every brace, parenthesis and bracket.
fn at_module_top_level(masked: &str, at: usize) -> bool {
    let mut depth = 0usize;
    for c in masked[..at].chars() {
        match c {
            '{' | '(' | '[' => depth += 1,
            '}' | ')' | ']' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    depth == 0
}

/// `true` when the masked module declares `export const name = { ... };`
/// at its top level and the object holds only [`plain_methods`]. A module
/// cannot export a second `name`, so the test's import of `name` is this
/// object. A type annotation (`export const name: T =`) is refused because
/// its `=` is not read here.
fn exports_top_level_const(masked: &str, name: &str) -> bool {
    masked.match_indices("export").any(|(at, _)| {
        if !word_boundary_before(masked, at) || !at_module_top_level(masked, at) {
            return false;
        }
        let rest = &masked[at + "export".len()..];
        strip_required_space(rest)
            .and_then(|rest| rest.strip_prefix("const"))
            .and_then(strip_required_space)
            .and_then(|rest| rest.strip_prefix(name))
            .map(str::trim_start)
            .filter(|rest| !rest.starts_with("=="))
            .and_then(|rest| rest.strip_prefix('='))
            .map(str::trim_start)
            .and_then(|rest| {
                let end = closing_offset(rest, '{', '}').filter(|_| rest.starts_with('{'))?;
                let after = rest[end..].trim_start();
                (after.is_empty() || after.starts_with(';') || after.starts_with('\n'))
                    .then(|| &rest[1..end - 1])
            })
            .is_some_and(plain_methods)
    })
}

/// `true` when the masked object-literal `body` is only method shorthands
/// `m(params): T { ... }` whose bodies make no call and name nothing but
/// their parameters and [`OWNER_WORDS`] outside member names. The spy
/// wraps the original method, so a method that calls anything (itself
/// through `this` or the object, a helper that does, a getter or a
/// `toString`) could record the asserted payload whatever the owner sent.
/// A spread, computed key or property value is refused for the same
/// reason: it can hand the object an existing mock or a function that
/// reaches the spy.
fn plain_methods(body: &str) -> bool {
    if body.contains("...") || body.contains(['[', '`']) || !body.is_ascii() {
        return false;
    }
    let mut rest = body.trim_start();
    while !rest.is_empty() {
        let name_len = rest
            .find(|c: char| !is_identifier_part(c))
            .unwrap_or(rest.len());
        if name_len == 0 || !rest.starts_with(is_identifier_start) {
            return false;
        }
        let after_name = rest[name_len..].trim_start();
        let Some(params_end) = after_name
            .starts_with('(')
            .then(|| closing_offset(after_name, '(', ')'))
            .flatten()
        else {
            return false;
        };
        if after_name[1..params_end - 1].contains(['=', '(']) {
            return false;
        }
        let params: Vec<&str> = after_name[1..params_end - 1]
            .split(',')
            .filter_map(|param| words(param).next())
            .collect();
        let after_params = &after_name[params_end..];
        let Some(open) = after_params.find('{') else {
            return false;
        };
        let header = after_params[..open].trim();
        if !header.is_empty() && !header.starts_with(':') {
            return false;
        }
        let Some(body_end) = closing_offset(&after_params[open..], '{', '}') else {
            return false;
        };
        let method_body = &after_params[open + 1..open + body_end - 1];
        if method_body.contains('(')
            || !words_outside_members(method_body)
                .all(|word| OWNER_WORDS.contains(&word) || params.contains(&word))
        {
            return false;
        }
        rest = after_params[open + body_end..].trim_start();
        rest = match rest.strip_prefix(',') {
            Some(next) => next.trim_start(),
            None if rest.is_empty() => rest,
            None => return false,
        };
    }
    true
}

/// Words of masked `text` that are not member names (`a.b` yields `a`).
fn words_outside_members(text: &str) -> impl Iterator<Item = &str> {
    let mut at = 0usize;
    text.split(|c: char| !is_identifier_part(c))
        .filter(move |word| {
            let start = at;
            at += word.len() + 1;
            word.starts_with(is_identifier_start) && !text[..start].trim_end().ends_with('.')
        })
}

/// `true` when the test file installs exactly one spy and runs no test
/// twice and replaces no module: `spyOn` appears once in the raw text
/// (so a commented copy cannot stand in for the real one) and no `\u`
/// escape could spell another, and the file holds no `each`,
/// `concurrent`, `retryTimes`, `mock`, `doMock`, module reset or
/// isolation, `require`, dynamic `import(` or `eval`.
pub(super) fn spy_safe_test_file(source: &str) -> bool {
    const RUNS_OR_REPLACES: [&str; 12] = [
        "each",
        "concurrent",
        "retryTimes",
        "mock",
        "doMock",
        "unstable_mockModule",
        "resetModules",
        "isolateModules",
        "isolateModulesAsync",
        "require",
        "requireActual",
        "eval",
    ];
    let masked = ts_mask_literals_and_comments(source);
    source.matches("spyOn").count() == 1
        && one_test_after_imports(&masked)
        && imports_bind_names(&masked)
        && !source.contains("\\u")
        && !masked.contains("import(")
        && RUNS_OR_REPLACES
            .iter()
            .all(|word| identifier_or_member_offsets(&masked, word) == 0)
}

/// `true` when the masked file's top level is named imports (`import ...
/// from "..."`) and then exactly one `test(...)` or `it(...)` call. The
/// spy wraps whatever `object.method` holds when the test body runs, and
/// `spyOn` on a method that is already a mock returns that mock with its
/// earlier calls; so no hook, earlier test, top-level statement or
/// side-effect import may run first and leave a mock with calls there.
fn one_test_after_imports(masked: &str) -> bool {
    let mut top = String::new();
    let mut depth = 0usize;
    for c in masked.chars() {
        match c {
            '{' | '(' | '[' => {
                if depth == 0 {
                    top.push(c);
                }
                depth += 1;
            }
            '}' | ')' | ']' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    top.push(c);
                }
            }
            _ if depth == 0 => top.push(c),
            _ => {}
        }
    }
    let mut tests = 0usize;
    for statement in top.split([';', '\n']).map(str::trim) {
        if statement.is_empty() {
            continue;
        }
        let is_named_import = statement
            .strip_prefix("import")
            .is_some_and(|rest| rest.starts_with([' ', '{']))
            && statement.contains(" from ");
        if is_named_import {
            if tests > 0 {
                return false;
            }
            continue;
        }
        if !matches!(statement, "test()" | "it()") {
            return false;
        }
        tests += 1;
    }
    tests == 1
}

/// `true` when every import clause of the masked file binds a name that is
/// not a `type` one: `import {} from "x"` and `import { type T } from "x"`
/// bind nothing at run time yet still load the module (under
/// `verbatimModuleSyntax` or Babel), and the import list the source check
/// reads would not hold them.
fn imports_bind_names(masked: &str) -> bool {
    masked.match_indices("import").all(|(at, _)| {
        if !word_boundary_before(masked, at) {
            return true;
        }
        let clause = masked[at + "import".len()..]
            .split(" from ")
            .next()
            .unwrap_or_default();
        let inner = clause
            .split_once('{')
            .map_or("", |(_, inner)| inner.split('}').next().unwrap_or_default());
        !words(clause).any(|word| word == "type")
            && (!clause.contains('{') || inner.trim_start().starts_with(is_identifier_start))
    })
}

/// Whole-word occurrences of `name`, member names included.
fn identifier_or_member_offsets(masked: &str, name: &str) -> usize {
    masked
        .match_indices(name)
        .filter(|(at, _)| {
            !masked[..*at]
                .chars()
                .next_back()
                .is_some_and(is_identifier_part)
                && !masked[at + name.len()..].starts_with(is_identifier_part)
        })
        .count()
}

/// `true` when `test_body` (in `test_file`, whose spy targets are
/// `targets`) declares `const|let|var spy = jest.spyOn(local, "method");`
/// at its callback's top level for a target of this file, mentions `spy`
/// nowhere else but `expect(spy)`, names `local` only there, calls the
/// owner exactly once, and spells no word but those of its declaration,
/// the owner call (with literal arguments), `expect` and
/// `toHaveBeenCalledWith`. Any other word could be a helper call, a
/// constructor, an `await` that lets other work run, or a getter that
/// reaches the spied method.
pub(super) fn credits_spy(
    targets: &[SpyTarget],
    owner_name: &str,
    spy: &str,
    test_file: &Path,
    test_body: &str,
) -> bool {
    let masked_body = ts_mask_literals_and_comments(test_body);
    let Some(range) = callback_scope_range(&masked_body) else {
        return false;
    };
    let scope = &masked_body[range];
    // A tagged template calls its tag without a `(`; a non-ASCII
    // identifier is no word `words` can check.
    if scope.contains('`')
        || !scope.is_ascii()
        || identifier_mentions_outside_expect(test_body, spy) != 1
    {
        return false;
    }
    let owner_calls = calls_in(scope)
        .iter()
        .filter(|call| call.name == owner_name)
        .count();
    if owner_calls != 1 {
        return false;
    }
    targets
        .iter()
        .filter(|target| target.test_file == test_file)
        .any(|target| {
            let allowed = [
                "const",
                "let",
                "var",
                spy,
                target.local.as_str(),
                "jest",
                "vi",
                "spyOn",
                owner_name,
                "expect",
                "toHaveBeenCalledWith",
                "true",
                "false",
                "null",
            ];
            words(scope).all(|word| allowed.contains(&word))
                && identifier_offsets(scope, &target.local).len() == 1
                && declares_spy(scope, spy, &target.local)
                && ["\"", "'"].iter().any(|quote| {
                    let method = format!("{quote}{}{quote}", target.method);
                    [", ", ","].iter().any(|comma| {
                        test_body.contains(&format!("spyOn({}{comma}{method})", target.local))
                    })
                })
        })
}

/// Every identifier-like word of masked `text`, member names included;
/// number literals are skipped.
fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !is_identifier_part(c))
        .filter(|word| word.starts_with(is_identifier_start))
}

/// `true` when the masked callback `scope` declares, at its top level,
/// `const|let|var spy = jest.spyOn(local, ...` (or `vi.spyOn`). The method
/// name is read from the raw text: the file has one `spyOn`.
fn declares_spy(scope: &str, spy: &str, local: &str) -> bool {
    ["const", "let", "var"].iter().any(|keyword| {
        scope.match_indices(keyword).any(|(at, _)| {
            if !word_boundary_before(scope, at) || !at_top_level(scope, at) {
                return false;
            }
            let Some(rest) = strip_required_space(&scope[at + keyword.len()..]) else {
                return false;
            };
            let Some(rest) = rest.strip_prefix(spy) else {
                return false;
            };
            let Some(rest) = rest.trim_start().strip_prefix('=') else {
                return false;
            };
            let rest = rest.trim_start();
            let Some(rest) = ["jest.spyOn(", "vi.spyOn("]
                .iter()
                .find_map(|head| rest.strip_prefix(head))
            else {
                return false;
            };
            rest.trim_start()
                .strip_prefix(local)
                .is_some_and(|rest| rest.trim_start().starts_with(','))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The owner extractor yields no nested function owner today, so this
    /// reads the guard directly: an owner below the module's top level may
    /// see its own `mailer`, not the exported one the test spies on.
    #[test]
    fn module_object_call_needs_an_owner_at_the_module_top_level() {
        let owner =
            "function announceMail(id: number): void {\n    mailer.send(\"order \" + id);\n  }";
        let line = "    mailer.send(\"order \" + id);";
        let id = ["id".to_string()];
        let top =
            format!("export const mailer = {{ send(m: string) {{ return m; }} }};\n{owner}\n");
        let nested = format!(
            "export const mailer = {{ send(m: string) {{ return m; }} }};\nexport function wrap(mailer: Mailer) {{\n  {owner}\n}}\n"
        );
        assert_eq!(
            module_object_calls(line, owner, "announceMail", &id, 2, &top),
            vec![ModuleObjectCall {
                object: "mailer".to_string(),
                method: "send".to_string(),
            }]
        );
        assert!(module_object_calls(line, owner, "announceMail", &id, 3, &nested).is_empty());
        assert!(module_object_calls(line, owner, "announceMail", &id, 0, &top).is_empty());
    }
}

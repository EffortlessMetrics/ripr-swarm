//! Whether one related test reaches the owner ONLY through direct, recognized
//! owner calls (#5491, RIPR-SPEC-0224). `missing_input` claims that every input
//! the row feeds the owner misses the boundary, which is false if the owner is
//! also reached through a helper, alias, `map`/`partial` reference, bound method
//! or a stand-in receiver that the call reader does not model. This reads the
//! test body textually and fails closed: any owner mention that is not a direct
//! call, any call to a callee it cannot vouch for, and any dynamic construct
//! return `false`.

use super::related_tests::builtin_is_shadowed;
use super::{PythonOwner, PythonTest};
use crate::domain::OwnerKind;

/// Names a test may call that cannot reach project code.
pub(super) const SAFE_BARE_CALLS: &[&str] = &[
    "abs",
    "all",
    "any",
    "bool",
    "dict",
    "float",
    "int",
    "len",
    "list",
    "max",
    "min",
    "range",
    "repr",
    "round",
    "set",
    "sorted",
    "str",
    "sum",
    "tuple",
    "type",
    "isinstance",
    "print",
];

/// Dynamic constructs that can reach the owner without naming it as a call.
const DYNAMIC_WORDS: &[&str] = &[
    "getattr",
    "eval",
    "exec",
    "globals",
    "locals",
    "importlib",
    "__import__",
    "lambda",
    "partial",
    "usefixtures",
    "builtins",
    "__builtins__",
];

/// The `unittest.TestCase` methods a test may call without reaching project
/// code through them.
const UNITTEST_API: &[&str] = &[
    "assertEqual",
    "assertNotEqual",
    "assertTrue",
    "assertFalse",
    "assertIs",
    "assertIsNot",
    "assertIsNone",
    "assertIsNotNone",
    "assertIn",
    "assertNotIn",
    "assertIsInstance",
    "assertNotIsInstance",
    "assertRaises",
    "assertRaisesRegex",
    "assertWarns",
    "assertWarnsRegex",
    "assertLogs",
    "assertNoLogs",
    "assertAlmostEqual",
    "assertNotAlmostEqual",
    "assertGreater",
    "assertGreaterEqual",
    "assertLess",
    "assertLessEqual",
    "assertRegex",
    "assertNotRegex",
    "assertCountEqual",
    "assertMultiLineEqual",
    "assertSequenceEqual",
    "assertListEqual",
    "assertTupleEqual",
    "assertSetEqual",
    "assertDictEqual",
    "subTest",
    "fail",
    "skipTest",
];

const KEYWORDS: &[&str] = &[
    "assert", "and", "or", "not", "in", "is", "if", "else", "elif", "for", "while", "with", "as",
    "return", "def", "class", "raise", "yield", "del", "from", "import", "try", "except",
    "finally", "pass", "lambda", "None", "True", "False", "print",
];

pub(super) fn row_reaches_owner_only_by_direct_calls(
    owner: &PythonOwner,
    test: &PythonTest,
    call_names: &[String],
) -> bool {
    // Code outside the test function (autouse fixture, `pytestmark`, setup or
    // teardown hook, module-level call, conftest fixture or hook) can reach the
    // owner implicitly; existence alone withholds the claim. A name the test
    // module rebinds or defines (`def can_reserve`, `can_reserve = ...`) is not
    // the owner. A decorated owner (anything but staticmethod/classmethod)
    // may wrap or replace the body.
    if test.constant_rebinding.implicit_reach
        || test.constant_rebinding.opaque
        || call_names
            .iter()
            .chain([&owner.name])
            .flat_map(|name| {
                // The name and its bare import alias root (`s` of `s.owner`).
                [Some(name.as_str()), name.split('.').next()]
            })
            .flatten()
            .any(|name| {
                test.constant_rebinding.module_rebinds(name)
                    || test.constant_rebinding.rebinds(name)
            })
        || owner.decorators.iter().any(|decorator| {
            !(decorator.ends_with("staticmethod") || decorator.ends_with("classmethod"))
        })
    {
        return false;
    }
    // Only inert marks may decorate the test: any other decorator can wrap it
    // and call the owner.
    if !test
        .decorators
        .iter()
        .all(|decorator| is_inert_decorator(decorator))
    {
        return false;
    }
    if !test.fixtures.is_empty()
        || test.decorators.iter().any(|decorator| {
            decorator.contains("usefixtures")
                || call_names.iter().any(|name| contains_word(decorator, name))
                || contains_word(decorator, &owner.name)
        })
    {
        return false;
    }
    // `import builtins` can rewrite what a bare builtin name means.
    if test
        .imports
        .iter()
        .any(|import| import.imported == "builtins" || import.source_module == "builtins")
    {
        return false;
    }
    let Some(code) = code_only(&test.body_text) else {
        return false;
    };
    let plain_locals = plain_locals(&code, test);
    if DYNAMIC_WORDS.iter().any(|word| contains_word(&code, word)) {
        return false;
    }
    // A callee read out of a subscript or a call result (`H['x'](4, 4)`,
    // `make()(4, 4)`) is unknown.
    if code
        .match_indices([']', ')'])
        .any(|(at, _)| code[at + 1..].trim_start().starts_with('('))
    {
        return false;
    }
    let class_name = owner_class_name(owner);
    let bytes = code.as_bytes();
    let mut idx = 0usize;
    while idx < bytes.len() {
        let byte = bytes[idx];
        if !(byte == b'_' || byte.is_ascii_alphabetic()) {
            idx += 1;
            continue;
        }
        let start = idx;
        while idx < bytes.len() && (bytes[idx] == b'_' || bytes[idx].is_ascii_alphanumeric()) {
            idx += 1;
        }
        let ident = &code[start..idx];
        let prev = code[..start].chars().next_back();
        let next = code[idx..].trim_start().chars().next();
        let is_call = next == Some('(');
        let after_dot = prev == Some('.');
        if KEYWORDS.contains(&ident) && !after_dot {
            continue;
        }
        // The test's own `def name(` / `class name(` header is not a call.
        let head = code[..start].trim_end();
        if head.ends_with("def") || head.ends_with("class") {
            // A local definition of the owner's own name shadows it.
            if is_name_of_owner(owner, call_names, ident) {
                return false;
            }
            continue;
        }
        if !is_call && !after_dot && !is_name_of_owner(owner, call_names, ident) {
            continue;
        }
        let owner_name = is_name_of_owner(owner, call_names, ident);
        if owner_name {
            if !is_call {
                return false;
            }
            match owner.owner_kind {
                Some(OwnerKind::Function) => {
                    if after_dot && !qualified_call_is_owner(&code, start, ident, call_names) {
                        return false;
                    }
                }
                // An instance method is reached through a constructor whose body
                // (`__init__`, `__new__`, `__post_init__`) this guard cannot
                // read, so it is never claimed.
                Some(OwnerKind::Method) => return false,
                Some(OwnerKind::ClassMethod) => {
                    if !after_dot || !receiver_is_owner_class(&code, start, &class_name) {
                        return false;
                    }
                }
                _ => return false,
            }
            continue;
        }
        if !is_call {
            continue;
        }
        // An after-dot API name is trusted only on an established receiver, and
        // a bare builtin only while the test does not rebind it.
        let safe = if after_dot {
            after_dot_api_is_trusted(&code, start, ident, test)
        } else {
            SAFE_BARE_CALLS.contains(&ident) && !builtin_is_shadowed(test, ident)
        };
        if !safe {
            return false;
        }
        // A trusted call must not carry a project callable by reference
        // (`pytest.raises(E, probe, ...)`, `sorted(x, key=probe)`): its
        // arguments may only be literals, safe builtins, builtin exceptions,
        // plain locals and nested calls (vetted on their own turn).
        let Some(args) = call_arguments(&code, idx) else {
            return false;
        };
        if !arguments_are_closed(args, test, &plain_locals) {
            return false;
        }
    }
    true
}

/// The argument text of the call whose name ends at `name_end`, up to its
/// matching parenthesis.
fn call_arguments(code: &str, name_end: usize) -> Option<&str> {
    let rest = &code[name_end..];
    let open = name_end + (rest.len() - rest.trim_start().len());
    if !code[open..].starts_with('(') {
        return None;
    }
    let mut depth = 0usize;
    for (offset, ch) in code[open..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&code[open + 1..open + offset]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Builtin exception classes a test may name uncalled (`pytest.raises(E)`).
const BUILTIN_EXCEPTIONS: &[&str] = &[
    "ArithmeticError",
    "AssertionError",
    "AttributeError",
    "DeprecationWarning",
    "Exception",
    "IndexError",
    "KeyError",
    "LookupError",
    "NotImplementedError",
    "OverflowError",
    "RuntimeError",
    "RuntimeWarning",
    "StopIteration",
    "TypeError",
    "UserWarning",
    "ValueError",
    "ZeroDivisionError",
];

fn is_literal_word(ident: &str) -> bool {
    matches!(
        ident,
        "True" | "False" | "None" | "and" | "or" | "not" | "in" | "is" | "if" | "else" | "for"
    )
}

fn is_unshadowed_builtin(test: &PythonTest, ident: &str) -> bool {
    (SAFE_BARE_CALLS.contains(&ident) || BUILTIN_EXCEPTIONS.contains(&ident))
        && !builtin_is_shadowed(test, ident)
}

/// Every identifier in `args` (apart from keyword labels and called names) is
/// a literal word, an unshadowed builtin or exception, or a plain local.
fn arguments_are_closed(args: &str, test: &PythonTest, plain_locals: &[String]) -> bool {
    let bytes = args.as_bytes();
    let mut idx = 0usize;
    while idx < bytes.len() {
        let byte = bytes[idx];
        if byte.is_ascii_digit() {
            // A numeric literal, including exponents and separators.
            while idx < bytes.len()
                && (bytes[idx] == b'_' || bytes[idx] == b'.' || bytes[idx].is_ascii_alphanumeric())
            {
                idx += 1;
            }
            continue;
        }
        if !(byte == b'_' || byte.is_ascii_alphabetic()) {
            idx += 1;
            continue;
        }
        let start = idx;
        while idx < bytes.len() && (bytes[idx] == b'_' || bytes[idx].is_ascii_alphanumeric()) {
            idx += 1;
        }
        let ident = &args[start..idx];
        let after = args[idx..].trim_start();
        // A called name is vetted by the caller's own scan.
        if after.starts_with('(') {
            continue;
        }
        // A keyword label (`key=`, `match=`) names no object.
        if after.starts_with('=') && !after.starts_with("==") {
            continue;
        }
        // An attribute read can name a project callable.
        if args[..start].trim_end().ends_with('.') {
            return false;
        }
        if is_literal_word(ident)
            || is_unshadowed_builtin(test, ident)
            || plain_locals.iter().any(|local| local == ident)
        {
            continue;
        }
        return false;
    }
    true
}

/// Locals the test assigns exactly once from an expression made only of
/// literals, unshadowed builtins and earlier plain locals, and never rebinds
/// through a loop, `with ... as`, `def` or walrus.
fn plain_locals(code: &str, test: &PythonTest) -> Vec<String> {
    let mut plain: Vec<String> = Vec::new();
    for line in code.lines() {
        let line = line.trim();
        let Some((target, rhs)) = line.split_once('=') else {
            continue;
        };
        if rhs.starts_with('=') || target.ends_with(['!', '<', '>', '=', '+', '-', '*', '/']) {
            continue;
        }
        let name = target.trim();
        if name.is_empty()
            || !name
                .chars()
                .all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
            || name.starts_with(|ch: char| ch.is_ascii_digit())
        {
            continue;
        }
        let assignments = code
            .lines()
            .filter(|other| {
                other.trim().strip_prefix(name).is_some_and(|rest| {
                    rest.trim_start().starts_with('=') && !rest.trim_start().starts_with("==")
                })
            })
            .count();
        let rebound = ["for ", "as ", "def ", "global ", "nonlocal "]
            .iter()
            .any(|marker| code.contains(&format!("{marker}{name}")))
            || code.contains(&format!("{name} :="))
            || code.contains(&format!("({name}:="));
        if assignments == 1 && !rebound && arguments_are_closed(rhs, test, &plain) {
            plain.push(name.to_string());
        }
    }
    plain
}

/// A framework API call on a receiver whose identity is established:
/// `self.assert*` inside a `unittest.TestCase` class (the module's non-test
/// methods and assertion-named helpers are already withheld), or
/// `raises`/`warns`/`approx` on a name that is the real `pytest` import and is
/// not rebound. Any other receiver is unknown.
fn after_dot_api_is_trusted(code: &str, name_start: usize, ident: &str, test: &PythonTest) -> bool {
    let before = code[..name_start].trim_end();
    let Some(before) = before.strip_suffix('.') else {
        return false;
    };
    let before = before.trim_end();
    let receiver_start = before
        .rfind(|ch: char| !(ch == '_' || ch.is_ascii_alphanumeric()))
        .map_or(0, |idx| idx + 1);
    let receiver = &before[receiver_start..];
    // A longer chain (`a.b.approx`), a call or a subscript result is unknown.
    if receiver.is_empty() || before[..receiver_start].ends_with(['.', ')', ']']) {
        return false;
    }
    if receiver == "self" {
        return test.framework == super::PythonFramework::Unittest
            && UNITTEST_API.contains(&ident)
            && !test.constant_rebinding.rebinds(ident)
            && !test.constant_rebinding.module_rebinds(ident);
    }
    matches!(ident, "raises" | "warns" | "approx")
        && test.imports.iter().any(|import| {
            import.alias == receiver
                && import.source_module.is_empty()
                && import.imported == "pytest"
        })
        && !test.constant_rebinding.rebinds(receiver)
        && !test.constant_rebinding.module_rebinds(receiver)
}

fn owner_class_name(owner: &PythonOwner) -> String {
    let path = if owner.class_path.is_empty() {
        owner
            .qualified_name
            .rsplit_once('.')
            .map_or("", |(class, _)| class)
    } else {
        owner.class_path.as_str()
    };
    path.rsplit('.').next().unwrap_or("").to_string()
}

fn is_name_of_owner(owner: &PythonOwner, call_names: &[String], ident: &str) -> bool {
    ident == owner.name
        || call_names
            .iter()
            .any(|name| name.rsplit('.').next() == Some(ident))
}

/// `module.owner(` where the dotted chain before the call is a recognized
/// module-qualified spelling of the owner.
fn qualified_call_is_owner(
    code: &str,
    name_start: usize,
    ident: &str,
    call_names: &[String],
) -> bool {
    let before = &code[..name_start];
    let chain_start = before
        .rfind(|ch: char| !(ch == '.' || ch == '_' || ch.is_ascii_alphanumeric()))
        .map_or(0, |idx| idx + 1);
    let qualified = format!("{}{ident}", &before[chain_start..]);
    call_names.contains(&qualified)
}

/// The receiver text before the `.` that precedes `name_start`.
fn receiver_before(code: &str, name_start: usize) -> &str {
    code[..name_start]
        .trim_end()
        .trim_end_matches('.')
        .trim_end()
}

fn receiver_is_owner_class(code: &str, name_start: usize, class_name: &str) -> bool {
    !class_name.is_empty() && receiver_before(code, name_start).ends_with(class_name) && {
        let receiver = receiver_before(code, name_start);
        let head = &receiver[..receiver.len() - class_name.len()];
        !head.ends_with(|ch: char| ch == '_' || ch == '.' || ch.is_ascii_alphanumeric())
    }
}

/// Marks and skips that cannot wrap the test body.
fn is_inert_decorator(decorator: &str) -> bool {
    decorator.starts_with("pytest.mark.")
        || matches!(
            decorator,
            "staticmethod"
                | "classmethod"
                | "unittest.skip"
                | "unittest.skipIf"
                | "unittest.skipUnless"
                | "unittest.expectedFailure"
        )
}

fn contains_word(text: &str, word: &str) -> bool {
    if word.is_empty() {
        return false;
    }
    let mut from = 0usize;
    while let Some(rel) = text[from..].find(word) {
        let start = from + rel;
        let end = start + word.len();
        let before = text[..start].chars().next_back();
        let after = text[end..].chars().next();
        let word_char = |c: char| c == '_' || c.is_alphanumeric();
        if !before.is_some_and(word_char) && !after.is_some_and(word_char) {
            return true;
        }
        from = end;
    }
    false
}

/// The body with comments removed and string contents blanked. None when a
/// string cannot be read as a plain literal (an f-string may hide a call).
fn code_only(body: &str) -> Option<String> {
    let chars: Vec<char> = body.chars().collect();
    let mut out = String::with_capacity(body.len());
    let mut idx = 0usize;
    while idx < chars.len() {
        let ch = chars[idx];
        if ch == '#' {
            while idx < chars.len() && chars[idx] != '\n' {
                idx += 1;
            }
            continue;
        }
        if ch == '\'' || ch == '"' {
            // A string prefix containing f/F can hide a call.
            let mut back = idx;
            while back > 0 && chars[back - 1].is_ascii_alphabetic() {
                back -= 1;
            }
            if chars[back..idx].iter().any(|c| matches!(c, 'f' | 'F')) {
                return None;
            }
            let triple = idx + 2 < chars.len() && chars[idx + 1] == ch && chars[idx + 2] == ch;
            let width = if triple { 3 } else { 1 };
            idx += width;
            out.push(ch);
            loop {
                if idx >= chars.len() {
                    return None;
                }
                if chars[idx] == '\\' {
                    idx += 2;
                    continue;
                }
                if chars[idx] == ch
                    && (!triple
                        || (idx + 2 < chars.len() && chars[idx + 1] == ch && chars[idx + 2] == ch))
                {
                    idx += width;
                    break;
                }
                if !triple && chars[idx] == '\n' {
                    return None;
                }
                idx += 1;
            }
            out.push(ch);
            continue;
        }
        out.push(ch);
        idx += 1;
    }
    Some(out)
}

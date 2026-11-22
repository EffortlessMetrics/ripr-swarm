//! Replaced class methods (#15950, #6980).
//!
//! A method owner reached through a constructed receiver (`const c = new
//! Cart(); c.apply(...)`) or through its class (`Cart.make(...)`) is credited
//! on the member name. When the test file replaces that member, the call runs
//! the replacement and the changed body never executes:
//!
//! ```ts
//! c.apply = () => 100;
//! vi.spyOn(Cart.prototype, 'apply').mockReturnValue(100);
//! sinon.stub(c, 'apply').returns(100);
//! mock.method(c, 'apply', () => 100);
//! ```
//!
//! [`method_replaced_in_test`] refuses the relation, fail-closed, when the
//! file names the member in a replacement call or a member assignment. A spy
//! that only calls through is refused as well: telling it from a fabricating
//! one across hooks is not attempted here, and a withheld relation is the
//! safe side.

use super::class_entries;
use super::classifier::balanced_close_offset;
use super::related_tests::is_javascript_identifier_char;
use super::tests_extract::second_argument_literal;
use super::types::TypeScriptTest;

/// Calls whose second argument names the member they replace or wrap:
/// `spyOn(obj, 'm')`, sinon `stub` / `replace` / `replaceGetter` /
/// `replaceSetter`, node:test `mock.method`, Jest `replaceProperty`.
const REPLACEMENT_CALLS: [&str; 7] = [
    "spyOn",
    "stub",
    "replace",
    "replaceGetter",
    "replaceSetter",
    "method",
    "replaceProperty",
];

/// `true` when the test's file may replace the member `name`.
///
/// Two sources, either refuses:
///
/// - the file-wide replacement names (`member_replacements_in_file`): a
///   `spyOn`/`stub`/`replace`/mock argument or a `.name =` target anywhere in
///   the file, hooks included. The any-name marker alone does not refuse an
///   ordinary method: a `.mock.calls` read or an index assignment sets it, and
///   refusing on it would withhold nearly every method relation;
/// - the test body itself: a member assignment, a `defineProperty` or a
///   prototype `delete` naming it, or a replacement call whose member
///   argument is the literal name. The body is read directly because the
///   any-name marker hides the file's names.
pub(crate) fn method_replaced_in_test(test: &TypeScriptTest, name: &str) -> bool {
    if test
        .member_replacements_in_file
        .iter()
        .any(|replaced| replaced == name)
    {
        return true;
    }
    // Read unmasked: `defineProperty(c, 'apply', ...)` names the member in a
    // string, and a match inside a comment or string only withholds.
    class_entries::assigns_member_named(&test.body_text, name)
        || bracket_assigns_member_named(&test.body_text, name)
        || replacement_call_names_member(&test.body_text, name)
}

/// `true` when `body` assigns through a literal bracket key naming the
/// member: `c['apply'] = fake`, `Cart.prototype["apply"] ||= fake`.
fn bracket_assigns_member_named(body: &str, name: &str) -> bool {
    ['\'', '"', '`'].iter().any(|quote| {
        let key = format!("[{quote}{name}{quote}]");
        body.match_indices(&key).any(|(idx, _)| {
            let rest = body[idx + key.len()..].trim_start();
            // A compound operator (`||=`, `??=`, `&&=`) is still a write.
            let rest = ["||", "??", "&&"]
                .iter()
                .find_map(|operator| rest.strip_prefix(operator))
                .unwrap_or(rest);
            rest.strip_prefix('=')
                .is_some_and(|after| !after.starts_with('=') && !after.starts_with('>'))
        })
    })
}

/// `true` when `body` holds a [`REPLACEMENT_CALLS`] call whose second
/// argument is the string literal `name`.
fn replacement_call_names_member(body: &str, name: &str) -> bool {
    REPLACEMENT_CALLS.iter().any(|word| {
        body.match_indices(word).any(|(idx, _)| {
            if body[..idx]
                .chars()
                .next_back()
                .is_some_and(is_javascript_identifier_char)
            {
                return false;
            }
            let Some(args) = body[idx + word.len()..].trim_start().strip_prefix('(') else {
                return false;
            };
            let Some(end) = balanced_close_offset(args) else {
                return false;
            };
            second_argument_literal(&args[..end]) == Some(name)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::{bracket_assigns_member_named, replacement_call_names_member};

    #[test]
    fn a_bracket_write_names_the_member_by_its_literal_key() {
        for body in [
            "c['apply'] = () => 90;",
            "Cart.prototype[\"apply\"] ||= fake;",
            "c[`apply`]= fake;",
        ] {
            assert!(bracket_assigns_member_named(body, "apply"), "{body}");
        }
        for body in [
            // A read, a comparison, another key, a computed key.
            "const f = c['apply'];",
            "if (c['apply'] === fake) {}",
            "c['total'] = fake;",
            "c[apply] = fake;",
        ] {
            assert!(!bracket_assigns_member_named(body, "apply"), "{body}");
        }
    }

    #[test]
    fn a_replacement_call_must_name_the_member_as_its_second_argument() {
        for body in [
            "vi.spyOn(Cart.prototype, 'apply').mockReturnValue(1);",
            "sinon.stub(c, \"apply\").returns(1);",
            "sandbox.replace(c, 'apply', fake);",
            "mock.method(c, 'apply', () => 1);",
            "t.mock.method (c, `apply`);",
            "jest.replaceProperty(c, 'apply', fake);",
        ] {
            assert!(replacement_call_names_member(body, "apply"), "{body}");
        }
        for body in [
            // Another member, the name as the object, a non-literal member.
            "vi.spyOn(c, 'total');",
            "vi.spyOn(apply, 'call');",
            "sinon.stub(c, name);",
            // Not one of the calls.
            "restub(c, 'apply');",
            "text.replace('apply', 'x');",
            "c.apply(1, 2);",
        ] {
            assert!(!replacement_call_names_member(body, "apply"), "{body}");
        }
    }
}

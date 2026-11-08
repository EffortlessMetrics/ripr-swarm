//! The line-local call lexer behind `contains_call_name` (#13592 follow-up):
//! a template substitution is code, and a `/*` inside a line comment or a
//! string opens no block comment.

use super::*;

#[test]
fn a_call_inside_a_template_substitution_is_a_call() {
    assert!(contains_call_name("return `v=${render(x)}`;", "render"));
    assert!(contains_call_name("`${a}-${ `${render(x)}` }`", "render"));
    // Braces inside the substitution do not end it early.
    assert!(contains_call_name("`${ { k: 1 }.k + render(x) }`", "render"));
    // After the template closes, code resumes.
    assert!(contains_call_name("`${a}` + render(x)", "render"));
}

#[test]
fn a_call_spelled_in_template_or_string_text_is_not_a_call() {
    assert!(!contains_call_name("`call render(x) by hand`", "render"));
    assert!(!contains_call_name("`${a} then render(x)`", "render"));
    assert!(!contains_call_name("'render(x)'", "render"));
    assert!(!contains_call_name("\"render(x)\"", "render"));
    assert!(!contains_call_name("// render(x)", "render"));
    // An escaped `\${` is template text, not a substitution.
    assert!(!contains_call_name("`\\${render(x)}`", "render"));
}

#[test]
fn a_block_comment_opener_in_a_line_comment_or_string_opens_nothing() {
    let after_line_comment = "// a trailing /* after a segment\nreturn render(x);";
    assert!(!inside_block_comment(
        after_line_comment,
        after_line_comment.len() - 10
    ));
    assert!(contains_call_name(after_line_comment, "render"));
    let after_string = "const glob = \"/*\";\nreturn render(x);";
    assert!(contains_call_name(after_string, "render"));
    let after_template = "const glob = `/*`;\nreturn render(x);";
    assert!(contains_call_name(after_template, "render"));
}

#[test]
fn a_real_block_comment_still_hides_its_calls() {
    assert!(!contains_call_name("/* render(x) */", "render"));
    assert!(!contains_call_name("/*\n  render(x);\n", "render"));
    assert!(contains_call_name("/* off */ render(x);", "render"));
    // A `//` inside an open block comment does not end it.
    assert!(!contains_call_name("/* see // note\n render(x);", "render"));
}

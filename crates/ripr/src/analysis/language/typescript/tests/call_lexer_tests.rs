//! The line-local call lexer behind `contains_call_name` (#13592 follow-up):
//! a template substitution is code, and a `/*` inside a line comment or a
//! string opens no block comment.

use super::*;

#[test]
fn a_call_inside_a_template_substitution_is_a_call() {
    assert!(contains_call_name("return `v=${render(x)}`;", "render"));
    assert!(contains_call_name("`${a}-${ `${render(x)}` }`", "render"));
    // Braces inside the substitution do not end it early.
    assert!(contains_call_name(
        "`${ { k: 1 }.k + render(x) }`",
        "render"
    ));
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
fn a_block_comment_opener_in_a_whole_line_comment_opens_nothing() {
    let after_line_comment = "// a trailing /* after a segment\nreturn render(x);";
    assert!(!inside_block_comment(
        after_line_comment,
        after_line_comment.len() - 10
    ));
    assert!(contains_call_name(after_line_comment, "render"));
    let indented = "  if (a) {\n    // see /* here\n    render(x);";
    assert!(contains_call_name(indented, "render"));
}

#[test]
fn a_block_comment_opener_in_a_plain_string_opens_nothing() {
    assert!(contains_call_name(
        "const glob = \"/*\";\nreturn render(x);",
        "render"
    ));
    assert!(contains_call_name(
        "match(path, '/api/*');\nrender(x);",
        "render"
    ));
    assert!(contains_call_name("const g = `/*`;\nrender(x);", "render"));
}

#[test]
fn an_escaped_slash_before_a_star_opens_nothing() {
    assert!(contains_call_name(
        "const r = /\\/*/;\nreturn render(x);",
        "render"
    ));
    assert!(contains_call_name(
        "s.replace(/^\\/*/, '');\nreturn render(x);",
        "render"
    ));
}

#[test]
fn a_real_block_comment_still_hides_its_calls() {
    assert!(!contains_call_name("/* render(x) */", "render"));
    assert!(!contains_call_name("/*\n  render(x);\n", "render"));
    assert!(contains_call_name("/* off */ render(x);", "render"));
    // A `//` inside an open block comment does not end it.
    assert!(!contains_call_name("/* see // note\n render(x);", "render"));
}

#[test]
fn an_uncertain_block_comment_opener_still_hides_its_calls() {
    // A regex ending in `\//` is not a line comment.
    assert!(!contains_call_name(
        "const re = /^https?:\\/\\//; /* legacy:\n  render(x);\n*/",
        "render"
    ));
    // A regex `"` opens no string, even when the comment text closes it.
    assert!(!contains_call_name(
        "s.replace(/\"/g, ''); /* old \"path\":\n  render(x);\n*/",
        "render"
    ));
    // A regex class holding `//` is not a line comment.
    assert!(!contains_call_name(
        "const re = /[ //]/; /* legacy:\n  render(x);\n*/",
        "render"
    ));
    // The closing backtick of a multi-line template opens nothing.
    assert!(!contains_call_name(
        "const t = `\nline\n`; /* until `fix` lands:\n  render(x);\n*/",
        "render"
    ));
    // A JSX apostrophe opens no string.
    assert!(!contains_call_name(
        "<p>Don't</p>; /* don't run:\n  render(x);\n*/",
        "render"
    ));
}

#[test]
fn a_regex_inside_a_template_substitution_fails_closed() {
    // The `{` in the regex must not keep the substitution open past `}`.
    assert!(!contains_call_name(
        "`${s.replace(/{/g, '')} then render(y)`",
        "render"
    ));
    // A division is not a regex: code after the template still reads.
    assert!(contains_call_name(
        "const s = `${a / 2}` + render(x);",
        "render"
    ));
    assert!(contains_call_name(
        "const s = `${f(a) / g[0]}` + render(x);",
        "render"
    ));
}

#[test]
fn a_block_comment_opener_after_code_on_its_line_fails_closed() {
    // A string after other quoting on its line, a JSX attribute or a trailing
    // comment holding `/*` is not read finely enough to trust, so the opener
    // still hides what follows.
    assert!(!contains_call_name(
        "<a title=\"/*\">x</a>;\nreturn render(x);",
        "render"
    ));
    assert!(!contains_call_name(
        "const re = /\"/; f(\"/*\");\nreturn render(x);",
        "render"
    ));
    assert!(!contains_call_name(
        "next(); // then /* more\nreturn render(x);",
        "render"
    ));
}

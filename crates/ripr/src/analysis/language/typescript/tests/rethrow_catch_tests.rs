//! An assertion inside a `try` whose handler only rethrows the caught error
//! still fails the test (#9685). Any handler that can swallow it keeps the
//! body's assertions withheld (#8242).

use super::*;

fn assertions_in(body: &str) -> Vec<String> {
    let path = Path::new("tests/lib.test.ts");
    let source = format!("test(\"t\", () => {{\n{body}\n}});\n");
    assert_eq!(parse_error_reason(path, &source), None, "{body}");
    let tests = extract_tests(path, &source);
    assert_eq!(tests.len(), 1, "{body}");
    tests[0]
        .assertions
        .iter()
        .map(|assertion| assertion.matcher.clone())
        .collect()
}

#[test]
fn a_handler_that_only_rethrows_the_caught_error_keeps_the_body_assertions() {
    let check = "expect(f(3, 7)).toBe(21);";
    for body in [
        format!("try {{ {check} }} catch (error) {{ throw error; }}"),
        format!("try {{ {check} }} catch (e) {{ throw (e); }}"),
    ] {
        assert_eq!(assertions_in(&body), vec!["toBe".to_string()], "{body}");
    }
}

#[test]
fn a_handler_that_can_swallow_or_replace_the_failure_withholds_the_body() {
    let check = "expect(f(3, 7)).toBe(21);";
    for body in [
        format!("try {{ {check} }} catch {{ }}"),
        format!("try {{ {check} }} catch (e) {{ }}"),
        format!("try {{ {check} }} catch {{ throw new Error('x'); }}"),
        format!("try {{ {check} }} catch (e) {{ throw new Error('x'); }}"),
        format!("try {{ {check} }} catch (e) {{ throw other; }}"),
        format!("try {{ {check} }} catch (e) {{ log(e); throw e; }}"),
        format!("try {{ {check} }} catch (e) {{ if (x) {{ throw e; }} }}"),
        format!("try {{ {check} }} catch (e) {{ throw e; }} finally {{ return; }}"),
        format!("try {{ {check} }} catch ({{ message }}) {{ throw message; }}"),
    ] {
        assert!(assertions_in(&body).is_empty(), "{body}");
    }
}

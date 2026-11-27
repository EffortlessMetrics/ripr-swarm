//! Public diff controls for #15770.
use super::{ExposureClass, Finding, TempRepo, test_fn};

const FREE: &str = "#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Money(pub i64);
pub fn f(a: Money, b: Money) -> Money {
    if a.0 > 100 {
        Money(b.0 + a.0)
    } else {
        Money(0)
    }
}";
const ADD: &str = "#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Money(pub i64);
impl std::ops::Add for Money {
    type Output = Money;
    fn add(self, b: Money) -> Money {
        if self.0 > 100 {
            Money(b.0 + self.0)
        } else {
            Money(0)
        }
    }
}";

fn finding(source: &str, old: &str, new: &str, body: &str) -> Result<Finding, String> {
    let (at, text) = source
        .lines()
        .enumerate()
        .find(|(_, line)| line.trim() == new)
        .ok_or_else(|| format!("missing edited expression {new}"))?;
    let line = at + 1;
    let patch = format!(
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -{line} +{line} @@\n-{}\n+{text}\n",
        text.replace(new, old)
    );
    TempRepo::with_patch(source, &test_fn(body), &patch)?.return_finding(line, new)
}

fn free(source: &str, test: &str) -> Result<Finding, String> {
    finding(source, "Money(a.0 + b.0)", "Money(b.0 + a.0)", test)
}

#[test]
fn tuple_and_primitive_missed_branches_read_weak() -> Result<(), String> {
    let primitive = "pub fn g(n: i64, m: i64) -> i64 {\n    if n > 100 {\n        m + n\n    } else {\n        0\n    }\n}";
    let scalar = finding(primitive, "n + m", "m + n", "assert_eq!(g(2, 3), 0);")?;
    assert_eq!(scalar.class, ExposureClass::WeaklyExposed);
    let tuple = free(FREE, "assert_eq!(f(Money(2), Money(3)), Money(0));")?;
    assert_eq!(tuple.class, ExposureClass::WeaklyExposed);
    assert!(tuple.ripr.infect.summary.contains("other branch"));
    Ok(())
}

#[test]
fn unresolved_or_reaching_calls_keep_their_credit() -> Result<(), String> {
    let missed = "assert_eq!(f(Money(2), Money(3)), Money(0));";
    let taken = "assert_eq!(f(Money(102), Money(3)), Money(105));";
    let unknown = "assert_eq!(f(Money(std::hint::black_box(102)), Money(3)), Money(105));";
    let shadow = "let a = Money(2);\n        let a = Money(102);\n        assert_eq!(f(a, Money(3)), Money(105));";
    for (source, body) in [
        (FREE.to_owned(), taken.to_owned()),
        (FREE.to_owned(), format!("{missed}\n        {taken}")),
        (FREE.to_owned(), unknown.to_owned()),
        (
            format!("{FREE}\nmod other {{ pub struct Money(pub i64); }}"),
            missed.to_owned(),
        ),
        (FREE.to_owned(), shadow.to_owned()),
        (
            FREE.replace("if a.0", "let a = Money(a.0 + 100);\n    if a.0"),
            "assert_eq!(f(Money(2), Money(3)), Money(105));".to_owned(),
        ),
    ] {
        let result = free(&source, &body)?;
        assert_eq!(result.class, ExposureClass::Exposed, "{source}\n{body}");
    }
    // The type and value namespaces can differ. This local function returns
    // the right type, but constructs a value that takes the edited branch.
    let counterfeit = "fn Money(n: i64) -> super::Money { super::Money(n + 100) }\n        assert_eq!(f(Money(2), Money(3)), super::Money(205));";
    assert_eq!(free(FREE, counterfeit)?.class, ExposureClass::Exposed);
    let two = FREE
        .replace("pub i64);", "pub i64, pub i64);")
        .replace("if a.0", "if a.1")
        .replace("Money(b.0 + a.0)", "Money(b.0 + a.0, 0)")
        .replace("Money(0)", "Money(0, 0)");
    let unsupported = finding(
        &two,
        "Money(a.0 + b.0, 0)",
        "Money(b.0 + a.0, 0)",
        "assert_eq!(f(Money(2, 102), Money(3, 0)), Money(5, 0));",
    )?;
    assert_eq!(unsupported.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn bound_add_operands_select_the_edited_branch() -> Result<(), String> {
    for (value, expected, class) in [
        (2, 0, ExposureClass::WeaklyExposed),
        (102, 105, ExposureClass::Exposed),
    ] {
        let body = format!(
            "let a = Money({value});\n        let b = Money(3);\n        assert_eq!(a + b, Money({expected}));"
        );
        let found = finding(ADD, "Money(self.0 + b.0)", "Money(b.0 + self.0)", &body)?;
        assert_eq!(found.class, class, "{body}");
        if value == 2 {
            assert!(found.ripr.infect.summary.contains("other branch"));
        }
    }
    Ok(())
}

#[test]
fn edited_else_and_mixed_parameter_types() -> Result<(), String> {
    let src = FREE
        .replace("b: Money", "b: i64")
        .replace("a.0 > 100", "a.0 <= 100")
        .replace("Money(b.0 + a.0)", "Money(0)")
        .replace("else {\n        Money(0)", "else {\n        Money(b + a.0)");
    for (n, expected, class) in [
        (2, 0, ExposureClass::WeaklyExposed),
        (102, 105, ExposureClass::Exposed),
    ] {
        let test = format!("assert_eq!(f(Money({n}), 3), Money({expected}));");
        assert_eq!(
            finding(&src, "Money(a.0 + b)", "Money(b + a.0)", &test)?.class,
            class
        );
    }
    Ok(())
}

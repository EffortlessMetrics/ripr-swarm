//! #9478: owner fn path values in `.map` iterator chains.

use super::super::path_value;
use super::*;

const INC: &str = "pub fn inc(x: u32) -> u32 {
    x + 1
}
";

const PATH_VALUE_TESTS: &str = "use demo::inc;

#[test]
fn mapped() {
    assert_eq!(vec![1, 2].into_iter().map(inc).collect::<Vec<_>>(), vec![2, 3]);
}
";

fn admitted(lib: &str, tests: &str) -> Vec<String> {
    let index = index(&[(LIB, lib), (TESTS, tests)]);
    let pin = establish(&index, "inc", "x + 1");
    assert!(pin.is_some(), "the owner must establish a pin");
    let Some(pin) = pin else {
        return Vec::new();
    };
    admitted_texts(&index, &pin)
}

#[test]
fn substitute_finds_inc_call() {
    let index = index(&[(LIB, INC), (TESTS, PATH_VALUE_TESTS)]);
    let owner = owner(&index, "inc");
    let test = index.tests().iter().find(|t| t.name == "mapped");
    assert!(test.is_some(), "the mapped test must be indexed");
    let Some(test) = test else {
        return;
    };
    let chain = "vec![1, 2].into_iter().map(inc).collect::<Vec<_>>()";
    let call = path_value::substitute_owner_call(test, chain, "vec![2, 3]", owner, &index);
    assert_eq!(call.as_deref(), Some("inc(1)"));
}

#[test]
fn map_path_value_in_mod_tests_is_pinned() {
    let lib = "pub fn inc(x: u32) -> u32 {
    x + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapped() {
        assert_eq!(vec![1, 2].into_iter().map(inc).collect::<Vec<_>>(), vec![2, 3]);
    }
}
";
    let index = index(&[(LIB, lib)]);
    let test = index.tests().at(0);
    assert_eq!(
        test.assertions[0].kind,
        crate::domain::OracleKind::ExactValue,
        "oracle kind for reveal pin"
    );
    let pin = establish(&index, "inc", "x + 1");
    assert!(pin.is_some(), "the owner must establish a pin");
    let Some(pin) = pin else {
        return;
    };
    assert_eq!(
        admitted_texts(&index, &pin),
        vec![
            "assert_eq!(vec![1, 2].into_iter().map(inc).collect::<Vec<_>>(), vec![2, 3]);"
                .to_string()
        ]
    );
}

#[test]
fn map_path_value_is_pinned() {
    assert_eq!(
        admitted(INC, PATH_VALUE_TESTS),
        vec![
            "assert_eq!(vec![1, 2].into_iter().map(inc).collect::<Vec<_>>(), vec![2, 3]);"
                .to_string()
        ]
    );
}

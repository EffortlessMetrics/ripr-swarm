//! #16060: rustc rejects a bare macro call whose name both a glob import and
//! the macro prelude supply (E0659, every edition), so a glob cannot silently
//! rebind a core prelude macro. A foreign glob reports no site for those
//! names and keeps its site for every other trusted name.

use super::*;

const CORE_PRELUDE: [&str; 9] = [
    "assert",
    "assert_eq",
    "assert_ne",
    "debug_assert",
    "debug_assert_eq",
    "debug_assert_ne",
    "matches",
    "panic",
    "unreachable",
];

fn site_names(source: &str) -> BTreeSet<String> {
    sites_in_widest_context(source, &assertion_context_macros())
        .into_iter()
        .map(|(name, _)| name)
        .collect()
}

#[test]
fn a_foreign_glob_reports_no_site_for_a_core_prelude_macro() {
    let names = assertion_context_macros();
    for name in CORE_PRELUDE {
        assert!(names.contains(&name), "{name} must be a trusted name");
    }
    for source in [
        "use rayon::prelude::*;\n",
        "pub use rmcp::{ErrorData, model::*};\n",
        "fn f() {\n    use other::*;\n}\n",
        "mod inner {\n    pub use dep::*;\n}\n",
    ] {
        let sites = site_names(source);
        for name in CORE_PRELUDE {
            assert!(!sites.contains(name), "{source:?} reported {name}");
        }
        // The glob still binds every other trusted name, `vec` and
        // `format` included (a `#![no_std]` prelude lacks them).
        for name in ["vec", "format", "assert_ok", "json"] {
            assert!(sites.contains(name), "{source:?} lost {name}: {sites:?}");
        }
    }
}

#[test]
fn every_other_binding_of_a_core_prelude_name_keeps_its_site() {
    for source in [
        "use other::assert_eq;\n",
        "use other::ignore as assert_eq;\n",
        "macro_rules! assert_eq { ($($t:tt)*) => {} }\n",
        "#[macro_use]\nextern crate other;\n",
        "#![no_implicit_prelude]\nuse other::*;\n",
        "#[no_implicit_prelude]\nmod inner {\n    use other::*;\n}\n",
    ] {
        assert!(
            site_names(source).contains("assert_eq"),
            "{source:?} must keep an assert_eq site"
        );
    }
}

#[test]
fn a_foreign_glob_does_not_refuse_the_standard_assertions() {
    let plain = "use demo::weight;\n#[test]\nfn weighs() {\n    assert!(weight(4) > 0);\n    assert_eq!(weight(4), 12);\n}\n";
    // Fixture premise: the test is credited with no glob anywhere.
    assert_eq!(weight_refusal(plain, &[]), None);
    assert_eq!(weight_refusal_nth(plain, &[], None, 1), None);
    for glob in [
        "use rayon::prelude::*;\n",
        "use rmcp::{ErrorData, model::*};\n",
    ] {
        let own = format!("{glob}{plain}");
        for nth in 0..2 {
            assert_eq!(weight_refusal_nth(&own, &[], None, nth), None, "{glob}");
            assert_eq!(
                weight_refusal_nth(plain, &[("src/server.rs", glob)], None, nth),
                None,
                "{glob} elsewhere"
            );
        }
    }
    // The same glob still refuses a test that calls a name it can bind.
    let with_vec = plain.replace("{\n    assert!", "{\n    let _ = vec![0u8];\n    assert!");
    assert!(with_vec.contains("let _ = vec![0u8];"));
    assert_eq!(weight_refusal(&with_vec, &[]), None);
    assert!(matches!(
        weight_refusal(&format!("use rayon::prelude::*;\n{with_vec}"), &[]),
        Some(AssertionRefusal::Syntax(
            AssertionContextRefusal::MacroBinding(name)
        )) if name == "vec"
    ));
    // Under `#![no_implicit_prelude]` a glob binds the name without error,
    // and the attribute keeps refusing.
    assert!(matches!(
        weight_refusal(
            &format!("#![no_implicit_prelude]\nuse rayon::prelude::*;\n{plain}"),
            &[]
        ),
        Some(AssertionRefusal::Syntax(
            AssertionContextRefusal::MacroBinding(_)
        ))
    ));
}

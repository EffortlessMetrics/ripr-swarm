//! #12078: a strong oracle from a test whose input selects another arm must
//! not grip a `match_arm` seam. The repo seam reads `ripr check`'s own arm
//! selection (RIPR-SPEC-0229), in the full and the compact grip paths alike.

use super::*;

const PRICES: &str = "pub enum Size {\n    Small,\n    Medium,\n    Large,\n}\n\n\
pub fn price_cents(size: Size) -> u32 {\n    match size {\n        Size::Small => 300,\n        \
Size::Medium => 450,\n        Size::Large => 500,\n    }\n}\n";

fn arm_evidence(
    test_body: &str,
    arm: &str,
) -> Result<(TestGripEvidence, TestGripEvidence, [SeamGripClass; 2]), String> {
    arm_evidence_in(PRICES, test_body, arm)
}

fn arm_evidence_in(
    source: &str,
    test_body: &str,
    arm: &str,
) -> Result<(TestGripEvidence, TestGripEvidence, [SeamGripClass; 2]), String> {
    let tests = format!("use prices::*;\n\n#[test]\nfn prices() {{\n{test_body}\n}}\n");
    arm_evidence_with_tests(source, &tests, arm)
}

fn arm_evidence_with_tests(
    source: &str,
    tests: &str,
    arm: &str,
) -> Result<(TestGripEvidence, TestGripEvidence, [SeamGripClass; 2]), String> {
    let files: Vec<(PathBuf, &str)> = vec![
        (PathBuf::from("src/lib.rs"), source),
        (PathBuf::from("tests/prices.rs"), tests),
    ];
    let mut index = index_from_files(&files)?;
    // The fixture crate is `prices`, so `use prices::*;` globs in the
    // workspace's own items, not a foreign crate's macros (#15022).
    index.package_names = ["prices".to_string()].into_iter().collect();
    let seams = inventory_seams_from_index(&[PathBuf::from("src/lib.rs")], &index);
    let seam = seams
        .iter()
        .find(|seam| seam.kind() == SeamKind::MatchArm && seam.expression().starts_with(arm))
        .ok_or_else(|| format!("expected the `{arm}` match-arm seam: {seams:?}"))?;
    let full = evidence_for_seam(seam, &index);
    let context = CompactGripContext::new(&index);
    let compact = compact_evidence_for_seam(seam, &context);
    let classes = [classify_seam(seam, &full), classify_seam(seam, &compact)];
    Ok((full, compact, classes))
}

fn missing_values(evidence: &TestGripEvidence) -> Vec<&str> {
    evidence
        .missing_discriminators
        .iter()
        .map(|fact| fact.value.as_str())
        .collect()
}

#[test]
fn an_arm_no_test_input_selects_is_weakly_gripped_with_its_pattern_missing() -> Result<(), String> {
    let (full, compact, class) = arm_evidence(
        "    assert_eq!(price_cents(Size::Small), 300);\n    assert_eq!(price_cents(Size::Large), 500);",
        "Size::Medium",
    )?;
    assert_eq!(class, [SeamGripClass::WeaklyGripped; 2]);
    for evidence in [&full, &compact] {
        assert_eq!(evidence.activate.state, StageState::Weak);
        assert_eq!(missing_values(evidence), ["Size::Medium"]);
        assert!(
            evidence
                .activate
                .summary
                .starts_with(crate::domain::ARM_UNSELECTED_REASON_PREFIX),
            "{}",
            evidence.activate.summary
        );
    }
    Ok(())
}

#[test]
fn an_arm_a_test_input_selects_keeps_its_strong_grip() -> Result<(), String> {
    let (full, compact, class) = arm_evidence(
        "    assert_eq!(price_cents(Size::Small), 300);\n    assert_eq!(price_cents(Size::Medium), 450);",
        "Size::Medium",
    )?;
    assert_eq!(class, [SeamGripClass::StronglyGripped; 2]);
    for evidence in [&full, &compact] {
        assert_eq!(evidence.activate.state, StageState::Yes);
        assert_eq!(missing_values(evidence), Vec::<&str>::new());
    }
    Ok(())
}

#[test]
fn an_input_the_reading_cannot_resolve_leaves_the_arm_unnamed() -> Result<(), String> {
    // A computed input may select the arm, so the shared reading refuses and
    // the seam keeps the stage it had before #12078.
    let (full, compact, class) = arm_evidence(
        "    let size = if std::env::args().count() > 99 { Size::Small } else { Size::Medium };\n    \
         assert_eq!(price_cents(size), 450);\n    \
         assert_eq!(price_cents(Size::Large), 500);",
        "Size::Medium",
    )?;
    assert_eq!(class, [SeamGripClass::StronglyGripped; 2]);
    for evidence in [&full, &compact] {
        assert_eq!(evidence.activate.state, StageState::Yes);
        assert!(
            !missing_values(evidence).contains(&"Size::Medium"),
            "{:?}",
            evidence.missing_discriminators
        );
    }
    Ok(())
}

/// The owner re-enters itself, so a call that selects `Small` runs the
/// `Medium` arm too and its assertion observes that arm's value.
const RECURSIVE: &str = "pub enum Size {\n    Small,\n    Medium,\n    Large,\n}\n\n\
pub fn price_cents(size: Size) -> u32 {\n    match size {\n        \
Size::Small => price_cents(Size::Medium),\n        Size::Medium => 450,\n        \
Size::Large => 500,\n    }\n}\n";

/// The same re-entry through a helper the owner calls by name.
const MUTUAL: &str = "pub enum Size {\n    Small,\n    Medium,\n    Large,\n}\n\n\
pub fn price_cents(size: Size) -> u32 {\n    match size {\n        \
Size::Small => medium_price(),\n        Size::Medium => 450,\n        \
Size::Large => 500,\n    }\n}\n\n\
fn medium_price() -> u32 {\n    price_cents(Size::Medium)\n}\n";

/// The same re-entry through a helper passed as a function value, which no
/// call name in the owner leads to.
const FUNCTION_VALUE: &str = "pub enum Size {\n    Small,\n    Medium,\n    Large,\n}\n\n\
pub fn price_cents(size: Size) -> u32 {\n    match size {\n        \
Size::Small => apply(medium_price),\n        Size::Medium => 450,\n        \
Size::Large => 500,\n    }\n}\n\n\
fn apply(f: fn() -> u32) -> u32 {\n    f()\n}\n\n\
fn medium_price() -> u32 {\n    price_cents(Size::Medium)\n}\n";

#[test]
fn an_owner_that_may_call_itself_leaves_the_arm_unnamed() -> Result<(), String> {
    for (label, source) in [
        ("recursive", RECURSIVE),
        ("mutual", MUTUAL),
        ("function value", FUNCTION_VALUE),
    ] {
        let (full, compact, class) = arm_evidence_in(
            source,
            "    assert_eq!(price_cents(Size::Small), 450);",
            "Size::Medium",
        )?;
        assert_ne!(class[0], SeamGripClass::WeaklyGripped, "{label}: {full:?}");
        assert_ne!(
            class[1],
            SeamGripClass::WeaklyGripped,
            "{label}: {compact:?}"
        );
        for evidence in [&full, &compact] {
            assert_eq!(evidence.activate.state, StageState::Yes, "{label}");
            assert_eq!(
                missing_values(evidence),
                Vec::<&str>::new(),
                "{label}: {:?}",
                evidence.missing_discriminators
            );
        }
    }
    Ok(())
}

/// A trait method runs from calls that never name it: here `.into()` in a
/// test outside the related set reaches `from` with `Size::Medium`.
const TRAIT_IMPL: &str = "pub enum Size {\n    Small,\n    Medium,\n    Large,\n}\n\n\
impl From<Size> for u32 {\n    fn from(size: Size) -> u32 {\n        match size {\n            \
Size::Small => 300,\n            Size::Medium => 450,\n            \
Size::Large => 500,\n        }\n    }\n}\n";

/// The same arms in an inherent method, which every caller names.
const INHERENT: &str = "pub enum Size {\n    Small,\n    Medium,\n    Large,\n}\n\n\
pub struct Prices;\n\n\
impl Prices {\n    pub fn from(size: Size) -> u32 {\n        match size {\n            \
Size::Small => 300,\n            Size::Medium => 450,\n            \
Size::Large => 500,\n        }\n    }\n}\n";

#[test]
fn a_trait_method_owner_leaves_the_arm_unnamed() -> Result<(), String> {
    let (full, compact, class) = arm_evidence_in(
        TRAIT_IMPL,
        "    assert_eq!(u32::from(Size::Small), 300);\n    assert_eq!(u32::from(Size::Large), 500);",
        "Size::Medium",
    )?;
    assert_ne!(class[0], SeamGripClass::WeaklyGripped, "{full:?}");
    assert_ne!(class[1], SeamGripClass::WeaklyGripped, "{compact:?}");
    for evidence in [&full, &compact] {
        assert_eq!(evidence.activate.state, StageState::Yes);
        assert_eq!(missing_values(evidence), Vec::<&str>::new());
    }
    // Control: the same arms and calls on an inherent method name the arm.
    let (full, compact, class) = arm_evidence_in(
        INHERENT,
        "    assert_eq!(Prices::from(Size::Small), 300);\n    assert_eq!(Prices::from(Size::Large), 500);",
        "Size::Medium",
    )?;
    assert_eq!(class, [SeamGripClass::WeaklyGripped; 2]);
    for evidence in [&full, &compact] {
        assert_eq!(missing_values(evidence), ["Size::Medium"]);
    }
    Ok(())
}

/// #15006 review: a production wrapper nothing calls cannot run the owner,
/// so it leaves the arm named.
const UNUSED_WRAPPER: &str = "pub enum Size {\n    Small,\n    Medium,\n    Large,\n}\n\n\
pub fn price_cents(size: Size) -> u32 {\n    match size {\n        Size::Small => 300,\n        \
Size::Medium => 450,\n        Size::Large => 500,\n    }\n}\n\n\
pub fn advertised_small_price() -> u32 {\n    price_cents(Size::Small)\n}\n";

/// The same wrapper with a caller: a test outside the related set may reach
/// the owner through `menu()`, so the arm stays unnamed.
const CALLED_WRAPPER: &str = "pub enum Size {\n    Small,\n    Medium,\n    Large,\n}\n\n\
pub fn price_cents(size: Size) -> u32 {\n    match size {\n        Size::Small => 300,\n        \
Size::Medium => 450,\n        Size::Large => 500,\n    }\n}\n\n\
pub fn price_of(size: Size) -> u32 {\n    price_cents(size)\n}\n\n\
pub fn menu() -> u32 {\n    price_of(Size::Medium)\n}\n";

#[test]
fn an_uncalled_wrapper_keeps_the_arm_named_and_a_called_one_does_not() -> Result<(), String> {
    let test_body = "    assert_eq!(price_cents(Size::Small), 300);\n    assert_eq!(price_cents(Size::Large), 500);";
    let (full, compact, class) = arm_evidence_in(UNUSED_WRAPPER, test_body, "Size::Medium")?;
    assert_eq!(class, [SeamGripClass::WeaklyGripped; 2]);
    for evidence in [&full, &compact] {
        assert_eq!(evidence.activate.state, StageState::Weak);
        assert_eq!(missing_values(evidence), ["Size::Medium"]);
    }
    let (full, compact, class) = arm_evidence_in(CALLED_WRAPPER, test_body, "Size::Medium")?;
    assert_ne!(class[0], SeamGripClass::WeaklyGripped, "{full:?}");
    assert_ne!(class[1], SeamGripClass::WeaklyGripped, "{compact:?}");
    for evidence in [&full, &compact] {
        assert_eq!(evidence.activate.state, StageState::Yes);
        assert_eq!(missing_values(evidence), Vec::<&str>::new());
    }
    Ok(())
}

/// A wrapper named only in a string a derive reads (`serde(default)`) runs
/// whenever the struct deserializes, so its call keeps the arm unnamed.
const STRING_NAMED_WRAPPER: &str = "pub enum Size {\n    Small,\n    Medium,\n    Large,\n}\n\n\
pub fn price_cents(size: Size) -> u32 {\n    match size {\n        Size::Small => 300,\n        \
Size::Medium => 450,\n        Size::Large => 500,\n    }\n}\n\n\
pub struct Order {\n    #[serde(default = \"medium_price\")]\n    pub cents: u32,\n}\n\n\
pub fn medium_price() -> u32 {\n    price_cents(Size::Medium)\n}\n";

/// An uncalled `Prices::inner`, declared in an uncalled `outer`, holds one
/// mention, which counts once, so the call in `menu()`, which `checkout()`
/// reaches, still keeps the arm unnamed.
const NESTED_DEAD_AND_LIVE_CALLER: &str = "pub enum Size {\n    Small,\n    Medium,\n    Large,\n}\n\n\
pub fn price_cents(size: Size) -> u32 {\n    match size {\n        Size::Small => 300,\n        \
Size::Medium => 450,\n        Size::Large => 500,\n    }\n}\n\n\
pub struct Prices;\n\n\
fn outer() {\n    impl Prices {\n        fn inner() -> u32 {\n            price_cents(Size::Medium)\n        }\n    }\n}\n\n\
pub fn menu() -> u32 {\n    price_cents(Size::Medium)\n}\n\n\
pub fn checkout() -> u32 {\n    menu()\n}\n";

/// A `#[no_mangle]` wrapper runs from code no file names it in.
const EXPORTED_WRAPPER: &str = "pub enum Size {\n    Small,\n    Medium,\n    Large,\n}\n\n\
pub fn price_cents(size: Size) -> u32 {\n    match size {\n        Size::Small => 300,\n        \
Size::Medium => 450,\n        Size::Large => 500,\n    }\n}\n\n\
#[no_mangle]\npub extern \"C\" fn medium_price() -> u32 {\n    price_cents(Size::Medium)\n}\n";

/// Two `medium_price` functions: a call to either reads as a mention of
/// both, so neither counts as never running.
const SAME_NAMED_WRAPPERS: &str = "pub enum Size {\n    Small,\n    Medium,\n    Large,\n}\n\n\
pub fn price_cents(size: Size) -> u32 {\n    match size {\n        Size::Small => 300,\n        \
Size::Medium => 450,\n        Size::Large => 500,\n    }\n}\n\n\
pub mod a {\n    pub fn medium_price() -> u32 {\n        crate::price_cents(crate::Size::Medium)\n    }\n}\n\n\
pub mod b {\n    pub fn medium_price() -> u32 {\n        crate::price_cents(crate::Size::Medium)\n    }\n}\n";

/// An uncalled function whose macro call expands to a live `impl Drop` that
/// runs the owner with `Size::Medium`.
const MACRO_IMPL_IN_UNCALLED_FUNCTION: &str = "pub enum Size {\n    Small,\n    Medium,\n    Large,\n}\n\n\
pub fn price_cents(size: Size) -> u32 {\n    match size {\n        Size::Small => 300,\n        \
Size::Medium => 450,\n        Size::Large => 500,\n    }\n}\n\n\
pub struct Big;\n\n\
macro_rules! reg {\n    ($f:ident) => {\n        impl Drop for Big {\n            \
fn drop(&mut self) {\n                $f(Size::Medium);\n            }\n        }\n    };\n}\n\n\
fn unused() {\n    reg!(price_cents);\n}\n";

/// #15006 review: a nested fn exported under a link name runs from
/// `Size::Small`'s extern call, though its uncalled parent never runs.
/// `opener` spells the attribute's `#[`, which may hold whitespace or a
/// comment.
fn spaced_export(opener: &str) -> String {
    format!(
        "pub enum Size {{\n    Small,\n    Medium,\n    Large,\n}}\n\n\
unsafe extern \"Rust\" {{\n    #[link_name = \"review_price_callback\"]\n    fn invoke_callback() -> u32;\n}}\n\n\
pub fn price_cents(size: Size) -> u32 {{\n    match size {{\n        \
Size::Small => unsafe {{ invoke_callback() }},\n        Size::Medium => 450,\n        \
Size::Large => 500,\n    }}\n}}\n\n\
fn unused_outer() {{\n    {opener}unsafe(export_name = \"review_price_callback\")]\n    \
pub fn hidden_price() -> u32 {{\n        price_cents(Size::Medium)\n    }}\n}}\n"
    )
}

/// A local `dbg!` shadows the standard macro and expands to a live `impl
/// Drop` that runs the owner with `Size::Medium`.
const REBOUND_READ_MACRO: &str = "pub enum Size {\n    Small,\n    Medium,\n    Large,\n}\n\n\
pub fn price_cents(size: Size) -> u32 {\n    match size {\n        Size::Small => 300,\n        \
Size::Medium => 450,\n        Size::Large => 500,\n    }\n}\n\n\
pub struct Big;\n\n\
macro_rules! dbg {\n    ($f:ident) => {\n        impl Drop for Big {\n            \
fn drop(&mut self) {\n                $f(Size::Medium);\n            }\n        }\n    };\n}\n\n\
fn unused() {\n    dbg!(price_cents);\n}\n";

/// `paste!` assembles a call to `medium_price` that no file spells whole.
const PASTED_CALL: &str = "pub enum Size {\n    Small,\n    Medium,\n    Large,\n}\n\n\
pub fn price_cents(size: Size) -> u32 {\n    match size {\n        Size::Small => 300,\n        \
Size::Medium => 450,\n        Size::Large => 500,\n    }\n}\n\n\
pub fn medium_price() -> u32 {\n    price_cents(Size::Medium)\n}\n\n\
pub fn menu() -> u32 {\n    paste::paste! { [<medium _price>]() }\n}\n\n\
pub fn checkout() -> u32 {\n    menu()\n}\n";

#[test]
fn a_string_named_or_live_caller_keeps_the_arm_unnamed() -> Result<(), String> {
    // Only `Size::Large` is asserted: in the callback and macro cases a
    // `Size::Small` call also runs the `Medium` arm, so its result differs.
    let test_body = "    assert_eq!(price_cents(Size::Large), 500);";
    for (label, source) in [
        ("string-named wrapper", STRING_NAMED_WRAPPER),
        (
            "nested dead beside a live caller",
            NESTED_DEAD_AND_LIVE_CALLER,
        ),
        ("exported wrapper", EXPORTED_WRAPPER),
        ("same-named wrappers", SAME_NAMED_WRAPPERS),
        (
            "macro-emitted impl in an uncalled function",
            MACRO_IMPL_IN_UNCALLED_FUNCTION,
        ),
        (
            "spaced export attribute in an uncalled function",
            spaced_export("# [").as_str(),
        ),
        (
            "comment-split export attribute in an uncalled function",
            spaced_export("# /* */ [").as_str(),
        ),
        (
            "contiguous export attribute in an uncalled function",
            spaced_export("#[").as_str(),
        ),
        (
            "rebound read macro in an uncalled function",
            REBOUND_READ_MACRO,
        ),
        ("pasted call to an uncalled function", PASTED_CALL),
    ] {
        let (full, compact, class) = arm_evidence_in(source, test_body, "Size::Medium")?;
        assert_ne!(class[0], SeamGripClass::WeaklyGripped, "{label}: {full:?}");
        assert_ne!(
            class[1],
            SeamGripClass::WeaklyGripped,
            "{label}: {compact:?}"
        );
        for evidence in [&full, &compact] {
            assert_eq!(evidence.activate.state, StageState::Yes, "{label}");
            assert_eq!(missing_values(evidence), Vec::<&str>::new(), "{label}");
        }
    }
    Ok(())
}

/// #15006 review: an `impl` declared inside an uncalled function is live,
/// and `Size::Small => Forward.into()` runs the `Medium` arm through it.
const IMPL_IN_UNCALLED_FUNCTION: &str = "pub enum Size {\n    Small,\n    Medium,\n    Large,\n}\n\n\
pub struct Forward;\n\n\
pub fn price_cents(size: Size) -> u32 {\n    match size {\n        Size::Small => Forward.into(),\n        \
Size::Medium => 450,\n        Size::Large => 500,\n    }\n}\n\n\
fn unused_outer() {\n    impl From<Forward> for u32 {\n        fn from(_: Forward) -> u32 {\n            \
price_cents(Size::Medium)\n        }\n    }\n}\n";

#[test]
fn an_impl_inside_an_uncalled_function_keeps_the_arm_unnamed() -> Result<(), String> {
    let (full, compact, class) = arm_evidence_in(
        IMPL_IN_UNCALLED_FUNCTION,
        "    assert_eq!(price_cents(Size::Small), 450);",
        "Size::Medium",
    )?;
    assert_ne!(class[0], SeamGripClass::WeaklyGripped, "{full:?}");
    assert_ne!(class[1], SeamGripClass::WeaklyGripped, "{compact:?}");
    for evidence in [&full, &compact] {
        assert_eq!(evidence.activate.state, StageState::Yes);
        assert_eq!(missing_values(evidence), Vec::<&str>::new());
    }
    Ok(())
}

/// #15006 review: a test file that imports the owner's name from a foreign
/// path may call that item, so its inputs say nothing about the owner's arms
/// (the check-side callee-identity defeat). A foreign import of another name
/// is the control: the arm stays named.
#[test]
fn a_foreign_import_of_the_owner_name_keeps_the_arm_unnamed() -> Result<(), String> {
    let body = "#[test]\nfn prices() {\n    assert_eq!(price_cents(Size::Small), 300);\n    \
assert_eq!(price_cents(Size::Large), 500);\n}\n";
    for (import, named) in [
        ("use other_prices::price_cents;\n", false),
        (
            "use other_prices::{discount, price_cents as price_cents};\n",
            false,
        ),
        ("use other_prices::discount;\n", true),
    ] {
        let tests = format!("use prices::*;\n{import}\n{body}");
        let (full, compact, class) = arm_evidence_with_tests(PRICES, &tests, "Size::Medium")?;
        let expected = if named {
            vec!["Size::Medium"]
        } else {
            Vec::new()
        };
        for (evidence, class) in [(&full, class[0]), (&compact, class[1])] {
            assert_eq!(missing_values(evidence), expected, "{import}");
            assert_eq!(
                class == SeamGripClass::WeaklyGripped,
                named,
                "{import}: {evidence:?}"
            );
        }
    }
    Ok(())
}

/// #15006 review: the compact path reads every related test for the arm,
/// not the rank-limited slice. Thirteen tests rank `t00`..`t12`, so `t12`
/// falls past the compact cutoff; its input alone decides the arm.
#[test]
fn a_test_past_the_compact_cutoff_still_decides_the_arm() -> Result<(), String> {
    for (last_input, named) in [("Size::Medium", false), ("Size::Large", true)] {
        let mut tests = String::from("use prices::*;\n");
        for idx in 0..12 {
            tests.push_str(&format!(
                "\n#[test]\nfn t{idx:02}() {{\n    assert_eq!(price_cents(Size::Small), 300);\n}}\n"
            ));
        }
        tests.push_str(&format!(
            "\n#[test]\nfn t12() {{\n    assert!(price_cents({last_input}) > 0);\n}}\n"
        ));
        let files: Vec<(PathBuf, &str)> = vec![
            (PathBuf::from("src/lib.rs"), PRICES),
            (PathBuf::from("tests/prices.rs"), tests.as_str()),
        ];
        let mut index = index_from_files(&files)?;
        index.package_names = ["prices".to_string()].into_iter().collect();
        let seams = inventory_seams_from_index(&[PathBuf::from("src/lib.rs")], &index);
        let seam = seams
            .iter()
            .find(|seam| {
                seam.kind() == SeamKind::MatchArm && seam.expression().starts_with("Size::Medium")
            })
            .ok_or_else(|| format!("expected the `Size::Medium` match-arm seam: {seams:?}"))?;
        let context = CompactGripContext::new(&index);
        // Fixture premise: thirteen related tests, and `t12` is outside the
        // compact slice.
        let limited = find_related_tests_compact(seam, &context);
        assert_eq!(limited.len(), COMPACT_RELATED_TEST_LIMIT);
        assert!(limited.iter().all(|related| related.test.name != "t12"));
        assert_eq!(find_related_tests_ranked(seam, &context).len(), 13);
        let full = evidence_for_seam(seam, &index);
        let compact = compact_evidence_for_seam(seam, &context);
        let expected = if named {
            vec!["Size::Medium"]
        } else {
            Vec::new()
        };
        for evidence in [&full, &compact] {
            assert_eq!(missing_values(evidence), expected, "{last_input}");
        }
        for evidence in [&full, &compact] {
            let class = classify_seam(seam, evidence);
            assert_eq!(
                class == SeamGripClass::WeaklyGripped,
                named,
                "{last_input}: {evidence:?}"
            );
        }
    }
    Ok(())
}

/// #15006 review reproduction: `use foreign::score;` makes `score(0)` a call
/// to a dependency, so production `score`'s `1 =>` arm keeps its stage. An
/// import of the same name from the owning crate (`scorer`) is the owner, so
/// the same test names `1` and weakens it.
#[test]
fn a_scalar_foreign_callee_keeps_the_arm_and_an_owning_crate_import_names_it() -> Result<(), String>
{
    const SCORE: &str = "pub fn score(x: u32) -> u32 {\n    match x {\n        0 => 10,\n        1 => 20,\n        _ => 30,\n    }\n}\n";
    for (import, named) in [("use foreign::score;", false), ("use scorer::score;", true)] {
        let tests =
            format!("{import}\n\n#[test]\nfn scores() {{\n    assert_eq!(score(0), 10);\n}}\n");
        let files: Vec<(PathBuf, &str)> = vec![
            (PathBuf::from("src/lib.rs"), SCORE),
            (PathBuf::from("tests/foreign.rs"), tests.as_str()),
        ];
        let mut index = index_from_files(&files)?;
        index.package_names = ["scorer".to_string()].into_iter().collect();
        let seams = inventory_seams_from_index(&[PathBuf::from("src/lib.rs")], &index);
        let seam = seams
            .iter()
            .find(|seam| seam.kind() == SeamKind::MatchArm && seam.expression().starts_with("1 =>"))
            .ok_or_else(|| format!("expected the `1 =>` match-arm seam: {seams:?}"))?;
        let context = CompactGripContext::new(&index);
        let full = evidence_for_seam(seam, &index);
        let compact = compact_evidence_for_seam(seam, &context);
        let expected = if named { vec!["1"] } else { Vec::new() };
        for evidence in [&full, &compact] {
            assert_eq!(missing_values(evidence), expected, "{import}: {evidence:?}");
            let weak = classify_seam(seam, evidence) == SeamGripClass::WeaklyGripped;
            assert_eq!(weak, named, "{import}: {evidence:?}");
            if !named {
                assert_eq!(evidence.activate.state, StageState::Yes, "{import}");
            }
        }
    }
    Ok(())
}

/// #15022: a test file that rebinds a read macro (`assert_eq!`) can run an
/// arm its visible input does not select. The rebound `assert_eq!` below
/// calls its first argument's function with `Size::Medium` and never spells
/// the owner's name, so no mention scan sees it.
#[test]
fn a_rebound_read_macro_keeps_the_arm_unnamed() -> Result<(), String> {
    let body = "#[test]\nfn prices() {\n    assert_eq!(price_cents(Size::Small), 300);\n    \
assert_eq!(price_cents(Size::Large), 500);\n}\n";
    for (prelude, named) in [
        (
            "macro_rules! assert_eq {\n    ($f:ident ( $($arg:tt)* ), $expected:expr) => {\n        \
let _ = $f(Size::Medium);\n    };\n}\n",
            false,
        ),
        ("", true),
    ] {
        let tests = format!("use prices::*;\n\n{prelude}\n{body}");
        let (full, compact, class) = arm_evidence_with_tests(PRICES, &tests, "Size::Medium")?;
        let expected = if named {
            vec!["Size::Medium"]
        } else {
            Vec::new()
        };
        for (evidence, class) in [(&full, class[0]), (&compact, class[1])] {
            assert_eq!(missing_values(evidence), expected, "{prelude}");
            assert_eq!(
                class == SeamGripClass::WeaklyGripped,
                named,
                "{prelude}: {evidence:?}"
            );
            // #15233: the refusal leaves activation unknown, not the generic
            // `yes` the visible `Small`/`Large` inputs would otherwise keep.
            let state = if named {
                StageState::Weak
            } else {
                StageState::Unknown
            };
            assert_eq!(evidence.activate.state, state, "{prelude}: {evidence:?}");
        }
    }
    Ok(())
}

/// #15233: the #15181 refusal fell back to the generic "Observed N concrete
/// activation value(s)" reading, which reads `yes`, so an arm no visible
/// input selects could grip strongly. Three test files over the same calls
/// (`Small` and `Large` asserted, `Medium` never passed): a test-local
/// `macro_rules! assert_eq` defined after every assertion (it never expands
/// there, but the whole-index rebound scan still sees it), one defined
/// before that runs the owner with `Size::Medium`, and the standard macro.
#[test]
fn a_rebound_read_macro_refusal_reads_activation_unknown_not_yes() -> Result<(), String> {
    const ASSERTS: &str = "        assert_eq!(price_cents(Size::Small), 300);\n        \
assert_eq!(price_cents(Size::Large), 500);\n";
    const REBIND: &str = "        macro_rules! assert_eq {\n            \
($f:ident ( $($arg:tt)* ), $e:expr) => {\n                let _ = $f(Size::Medium);\n            \
};\n        }\n";
    let file = |body: String| {
        format!(
            "#[cfg(test)]\nmod tests {{\n    use prices::*;\n\n    #[test]\n    \
fn small_and_large_prices() {{\n{body}    }}\n}}\n"
        )
    };
    for (label, tests, rebound) in [
        ("defined after", file(format!("{ASSERTS}{REBIND}")), true),
        ("defined before", file(format!("{REBIND}{ASSERTS}")), true),
        ("standard macro", file(ASSERTS.to_string()), false),
    ] {
        // Fixture premise: the three files differ only in the rebinding and
        // its place, and no file passes `Size::Medium` to the owner itself.
        assert_eq!(tests.matches("price_cents(Size::Small)").count(), 1);
        assert_eq!(tests.matches("price_cents(Size::Large)").count(), 1);
        assert!(!tests.contains("price_cents(Size::Medium)"), "{label}");
        assert_eq!(tests.contains("macro_rules! assert_eq"), rebound, "{label}");
        if rebound {
            let (definition, first_assert) = (
                tests.find("macro_rules!").unwrap_or(usize::MAX),
                tests.find("assert_eq!(price_cents").unwrap_or(usize::MAX),
            );
            assert_eq!(definition > first_assert, label == "defined after");
        }
        let (full, compact, class) = arm_evidence_with_tests(PRICES, &tests, "Size::Medium")?;
        for (evidence, class) in [(&full, class[0]), (&compact, class[1])] {
            // Premise: both visible inputs read as observed values.
            assert!(
                evidence
                    .activate
                    .summary
                    .contains("`Size::Large`, `Size::Small`"),
                "{label}: {}",
                evidence.activate.summary
            );
            if rebound {
                assert_eq!(evidence.activate.state, StageState::Unknown, "{label}");
                assert!(
                    evidence.activate.summary.starts_with(
                        "A read macro on the related tests' path (`assert_eq!`) is rebound"
                    ),
                    "{label}: {}",
                    evidence.activate.summary
                );
                assert_eq!(missing_values(evidence), Vec::<&str>::new(), "{label}");
                assert_eq!(class, SeamGripClass::ActivationUnknown, "{label}");
            } else {
                assert_eq!(evidence.activate.state, StageState::Weak, "{label}");
                assert_eq!(missing_values(evidence), ["Size::Medium"], "{label}");
                assert_eq!(class, SeamGripClass::WeaklyGripped, "{label}");
            }
            assert_ne!(class, SeamGripClass::StronglyGripped, "{label}");
        }
    }
    Ok(())
}

/// #15233 corpus capture: the rebinding inside the owner file's inline test
/// module (the issue's shape) also reads as a mention of the owner outside
/// the related tests' calls, a refusal that kept the generic `yes`. The
/// rebound check is asked before it, so the arm reads unknown there too. The
/// control without the rebinding names the arm.
#[test]
fn an_inline_test_module_rebinding_reads_activation_unknown() -> Result<(), String> {
    let rebind = "    macro_rules! assert_eq {\n        \
($f:ident ( $($arg:tt)* ), $expected:expr) => {\n            \
let _ = $f(Size::Medium);\n        };\n    }\n";
    for (prelude, rebound) in [(rebind, true), ("", false)] {
        let source = format!(
            "{PRICES}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n{prelude}\n    \
#[test]\n    fn small_and_large_prices() {{\n        \
assert_eq!(price_cents(Size::Small), 300);\n        \
assert_eq!(price_cents(Size::Large), 500);\n    }}\n}}\n"
        );
        // Fixture premise: the test sits in the owner's file, before or
        // without a rebinding, and never passes `Size::Medium` itself.
        assert!(source.contains("mod tests {\n    use super::*;"));
        assert!(!source.contains("price_cents(Size::Medium)"));
        assert_eq!(source.contains("macro_rules! assert_eq"), rebound);
        let (full, compact, class) =
            arm_evidence_with_tests(&source, "// no out-of-line tests\n", "Size::Medium")?;
        for (evidence, class) in [(&full, class[0]), (&compact, class[1])] {
            let summary = evidence.activate.summary.as_str();
            if rebound {
                assert_eq!(evidence.activate.state, StageState::Unknown, "{summary}");
                assert_eq!(class, SeamGripClass::ActivationUnknown, "{summary}");
                assert!(summary.starts_with("A read macro"), "{summary}");
            } else {
                assert_eq!(evidence.activate.state, StageState::Weak, "{summary}");
                assert_eq!(class, SeamGripClass::WeaklyGripped, "{summary}");
                assert_eq!(missing_values(evidence), ["Size::Medium"]);
            }
        }
    }
    Ok(())
}

/// #15233 review: the refusal summary names only the read macros that are
/// rebound. The test invokes a rebound `assert_eq!` and the standard
/// `println!`; only `assert_eq!` is named. The control, with the standard
/// `assert_eq!`, names the arm instead.
#[test]
fn the_rebound_refusal_names_only_the_rebound_read_macro() -> Result<(), String> {
    let body = "#[test]\nfn prices() {\n    println!(\"{}\", 1);\n    \
assert_eq!(price_cents(Size::Small), 300);\n    \
assert_eq!(price_cents(Size::Large), 500);\n}\n";
    let rebind = "macro_rules! assert_eq {\n    ($f:ident ( $($arg:tt)* ), $expected:expr) => {\n        \
let _ = $f(Size::Medium);\n    };\n}\n";
    for (prelude, rebound) in [(rebind, true), ("", false)] {
        let tests = format!("use prices::*;\n\n{prelude}\n{body}");
        // Fixture premise: both read macros are invoked, and only
        // `assert_eq` is ever rebound.
        assert!(tests.contains("println!(") && tests.contains("assert_eq!(price_cents"));
        assert!(!tests.contains("macro_rules! println"));
        assert_eq!(tests.contains("macro_rules! assert_eq"), rebound);
        let (full, compact, _) = arm_evidence_with_tests(PRICES, &tests, "Size::Medium")?;
        for evidence in [&full, &compact] {
            let summary = evidence.activate.summary.as_str();
            if rebound {
                assert_eq!(evidence.activate.state, StageState::Unknown, "{summary}");
                assert!(
                    summary.starts_with(
                        "A read macro on the related tests' path (`assert_eq!`) is rebound"
                    ),
                    "{summary}"
                );
                assert!(!summary.contains("println"), "{summary}");
            } else {
                assert_eq!(evidence.activate.state, StageState::Weak, "{summary}");
                assert_eq!(missing_values(evidence), ["Size::Medium"]);
            }
        }
    }
    Ok(())
}

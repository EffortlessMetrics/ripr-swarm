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
    let files: Vec<(PathBuf, &str)> = vec![
        (PathBuf::from("src/lib.rs"), source),
        (PathBuf::from("tests/prices.rs"), tests.as_str()),
    ];
    let index = index_from_files(&files)?;
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

#[test]
fn a_string_named_or_live_caller_keeps_the_arm_unnamed() -> Result<(), String> {
    let test_body = "    assert_eq!(price_cents(Size::Small), 300);\n    assert_eq!(price_cents(Size::Large), 500);";
    for (label, source) in [
        ("string-named wrapper", STRING_NAMED_WRAPPER),
        (
            "nested dead beside a live caller",
            NESTED_DEAD_AND_LIVE_CALLER,
        ),
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

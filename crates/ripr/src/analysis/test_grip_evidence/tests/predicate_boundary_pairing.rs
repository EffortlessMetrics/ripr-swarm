//! Repo repair grading must pair a predicate's boundary input with its oracle.

use super::*;

#[test]
fn only_an_exact_boundary_result_closes_the_predicate_gap() -> Result<(), String> {
    let source = "pub fn discount(qty: u32) -> u32 { if qty >= 10 { 5 } else { 0 } }";
    for (body, expected) in [
        ("discount(10);", SeamGripClass::WeaklyGripped),
        (
            "check(discount(1), 0, discount(10));",
            SeamGripClass::WeaklyGripped,
        ),
        (
            "check(discount(10), 5, discount(1));",
            SeamGripClass::StronglyGripped,
        ),
        (
            "check(prices::discount(10), 5, discount(1));",
            SeamGripClass::StronglyGripped,
        ),
        (
            "check(discount(10), discount(10), 0);",
            SeamGripClass::WeaklyGripped,
        ),
        ("assert!(discount(10) >= 0);", SeamGripClass::WeaklyGripped),
        (
            "assert!(discount(10).checked_add(1).is_some());",
            SeamGripClass::WeaklyGripped,
        ),
        (
            "discount(10);\nassert_eq!(discount(11), 5);",
            SeamGripClass::WeaklyGripped,
        ),
        ("assert_eq!(discount(9), 0);", SeamGripClass::WeaklyGripped),
        ("assert_eq!(discount(11), 5);", SeamGripClass::WeaklyGripped),
        (
            "assert_eq!(discount(10), 5);",
            SeamGripClass::StronglyGripped,
        ),
        (
            "let d = discount(10);\nassert_eq!(d, 5);",
            SeamGripClass::StronglyGripped,
        ),
    ] {
        let tests = format!(
            "use prices::discount;\nfn check(got: u32, want: u32, _unused: u32) {{\n    assert_eq!(got, want);\n}}\n#[test]\nfn low() {{ assert_eq!(discount(1), 0); }}\n\
             #[test]\nfn boundary() {{\n{body}\n}}\n"
        );
        let file = PathBuf::from("src/lib.rs");
        let mut index = index_from_files(&[
            (file.clone(), source),
            (PathBuf::from("tests/pricing.rs"), &tests),
        ])?;
        index.package_names = ["prices".to_string()].into_iter().collect();
        let seams = inventory_seams_from_index(&[file], &index);
        let seam = seams
            .iter()
            .find(|seam| {
                seam.kind() == SeamKind::PredicateBoundary && seam.expression() == "qty >= 10"
            })
            .ok_or("missing discount boundary")?;
        let context = CompactGripContext::new(&index);
        for evidence in [
            evidence_for_seam(seam, &index),
            compact_evidence_for_seam(seam, &context),
        ] {
            assert_eq!(
                classify_seam(seam, &evidence),
                expected,
                "{body}: {evidence:?}"
            );
        }
    }
    Ok(())
}

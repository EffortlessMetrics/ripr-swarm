//! #12940: the second-stage pool check must give the answer the complete
//! relation gives, without resolving owner calls for tests outside the pool.

use super::*;

const PRICING: &str = "pub fn discounted_total(amount: i32, threshold: i32) -> i32 \
    { if amount >= threshold { amount - 10 } else { amount } }\n\
    pub fn make_quote() -> i32 { 100 }\n";

const LABELS: &str = r#"
pub fn device_labels() -> Vec<&'static str> {
    Vec::new()
}

fn exercise_device_labels() -> Vec<&'static str> {
    device_labels()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helper_reaches_device_labels() {
        let labels = exercise_device_labels();
        assert!(labels.is_empty());
    }
}
"#;

fn fixture() -> Result<(FixtureIndex, Vec<RepoSeam>), String> {
    let files = [
        (PathBuf::from("src/pricing.rs"), PRICING),
        (PathBuf::from("src/labels.rs"), LABELS),
        (
            PathBuf::from("tests/direct.rs"),
            "#[test] fn boundary() { assert_eq!(discounted_total(100, 100), 90); }\n\
             #[test] fn arithmetic() { assert_eq!(2 + 2, 4); }\n",
        ),
        (
            PathBuf::from("tests/billing.rs"),
            "#[test] fn discounted_total_smoke() { assert_eq!(1, 1); }\n",
        ),
        (
            PathBuf::from("tests/integration.rs"),
            "#[test] fn quote_smoke() { let _ = make_quote(); assert!(true); }\n",
        ),
        (
            PathBuf::from("tests/integration_smoke.rs"),
            "#[test] fn smoke() { let _f = crate::pricing::discounted_total; assert_eq!(1, 1); }\n",
        ),
        (
            PathBuf::from("tests/contract_tests.rs"),
            r#"
#[test]
fn return_value_contract_mentions_empty_output() {
    let note = "device_labels(";
    let return_value = Vec::<&str>::new();
    assert!(!note.is_empty());
    assert!(return_value.is_empty());
}
"#,
        ),
    ];
    let index = index_from_files(&files)?;
    let seams = inventory_seams_from_index(
        &[
            PathBuf::from("src/pricing.rs"),
            PathBuf::from("src/labels.rs"),
        ],
        &index,
    );
    Ok((index, seams))
}

fn counted(reason: RelationReason) -> bool {
    !reason.is_proximity_only()
}

fn every_reason(_: RelationReason) -> bool {
    true
}

fn complete_answer(
    seam: &RepoSeam,
    context: &CompactGripContext<'_>,
    pool: &BTreeSet<usize>,
    counts: fn(RelationReason) -> bool,
) -> bool {
    !find_related_tests_in(seam, context, RelationScope::Pool(pool, counts)).is_empty()
}

#[test]
fn pool_check_matches_the_complete_relation_for_every_seam_and_pool() -> Result<(), String> {
    let (index, seams) = fixture()?;
    let context = CompactGripContext::new(&index);
    let test_count = context.tests.len();
    if test_count != 7 {
        return Err(format!("fixture should index 7 tests, got {test_count}"));
    }

    // The fixture must realize every counted reason, or agreement below
    // would say nothing about the routes that differ.
    let mut realized = BTreeSet::new();
    for seam in &seams {
        for (_, reason) in find_related_tests_with_context(seam, &context) {
            realized.insert(reason.as_str());
        }
    }
    for reason in [
        RelationReason::DirectOwnerCall,
        RelationReason::HelperOwnerCall,
        RelationReason::AssertionTargetAffinity,
        RelationReason::OwnerNamedTest,
        RelationReason::ImportPathAffinity,
        RelationReason::FixtureOwnerAffinity,
    ] {
        if !realized.contains(reason.as_str()) {
            return Err(format!(
                "fixture realizes no {} relation; got {realized:?}",
                reason.as_str()
            ));
        }
    }

    let mut pools = (0..test_count)
        .map(|index| BTreeSet::from([index]))
        .collect::<Vec<_>>();
    pools.push(BTreeSet::new());
    pools.push((0..test_count).collect());
    let mut related = 0usize;
    let mut unrelated = 0usize;
    for counts in [counted as fn(RelationReason) -> bool, every_reason] {
        for pool in &pools {
            // One memo per pool, shared by its seams as a narrowing pass
            // shares it, so later seams of an owner read the kept answer.
            let direct_calls = PoolDirectCalls::default();
            for seam in &seams {
                let expected = complete_answer(seam, &context, pool, counts);
                let actual = relates_to_pool_test(seam, &context, pool, counts, &direct_calls);
                if actual != expected {
                    return Err(format!(
                        "seam {} pool {pool:?}: pool check {actual}, complete relation {expected}",
                        seam.id().as_str()
                    ));
                }
                if expected {
                    related += 1;
                } else {
                    unrelated += 1;
                }
            }
        }
    }
    if related == 0 || unrelated == 0 {
        return Err(format!(
            "both answers must occur: related {related}, unrelated {unrelated}"
        ));
    }
    Ok(())
}

/// The pool tests `matcher` admits for the first seam of `file`, and the
/// same walk without a pool.
type OwnerCallMatcher = fn(
    &mut BTreeMap<usize, RelationReason>,
    &CompactGripContext<'_>,
    Option<&str>,
    &OwnerContext<'_>,
    Option<&BTreeSet<usize>>,
);

fn scoped_walk_admits_only_pool_tests(
    file: &str,
    matcher: OwnerCallMatcher,
    expected: RelationReason,
) -> Result<(), String> {
    let (index, seams) = fixture()?;
    let context = CompactGripContext::new(&index);
    let seam = seams
        .iter()
        .find(|seam| seam.file() == Path::new(file))
        .ok_or_else(|| format!("fixture needs a seam in {file}"))?;
    let owner = OwnerContext::resolve(seam, &context);

    let mut every = BTreeMap::new();
    matcher(&mut every, &context, None, &owner, None);
    let admitted = match every.iter().collect::<Vec<_>>().as_slice() {
        [(index, reason)] if **reason == expected => **index,
        _ => return Err(format!("one {expected:?} test expected, got {every:?}")),
    };

    let mut inside = BTreeMap::new();
    let pool = BTreeSet::from([admitted]);
    matcher(&mut inside, &context, None, &owner, Some(&pool));
    if inside != every {
        return Err(format!("pool {pool:?} admitted {inside:?}, not {every:?}"));
    }
    let others = (0..context.tests.len())
        .filter(|index| *index != admitted)
        .collect::<BTreeSet<_>>();
    let mut outside = BTreeMap::new();
    matcher(&mut outside, &context, None, &owner, Some(&others));
    if !outside.is_empty() {
        return Err(format!("a pool without the caller admitted {outside:?}"));
    }
    Ok(())
}

#[test]
fn direct_owner_call_scope_resolves_only_pool_tests() -> Result<(), String> {
    scoped_walk_admits_only_pool_tests(
        "src/pricing.rs",
        match_direct_owner_call,
        RelationReason::DirectOwnerCall,
    )
}

#[test]
fn helper_owner_call_scope_resolves_only_pool_tests() -> Result<(), String> {
    scoped_walk_admits_only_pool_tests(
        "src/labels.rs",
        match_helper_owner_call,
        RelationReason::HelperOwnerCall,
    )
}

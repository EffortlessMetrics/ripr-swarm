use crate::analysis::ClassifiedSeam;
use crate::analysis::seams::SeamGripClass;
use crate::output::agent_seam_packets::suggested_assertion_for_classified_seam;
use crate::output::path::display_path;
use crate::output::pilot::PilotCurrentChange;
use std::collections::BTreeMap;
use std::path::Path;

/// Rank actionable seams. Seams on lines the current change touches come
/// first; within each group (and when there is no change) the existing
/// class/evidence/location order holds.
pub(crate) fn top_actionable_seams<'a>(
    classified: &'a [ClassifiedSeam],
    max_seams: usize,
    current_change: Option<&PilotCurrentChange>,
) -> Vec<&'a ClassifiedSeam> {
    let in_change =
        |entry: &ClassifiedSeam| current_change.is_some_and(|change| change.touches(entry));
    let mut actionable = classified
        .iter()
        .filter(|entry| class_rank(entry.class).is_some())
        .map(|entry| (bool_rank(in_change(entry)), entry))
        .collect::<Vec<_>>();
    // The rank key holds `suggested_assertion_for_classified_seam`, which
    // derives repair-route readiness from the seam's evidence; computing it
    // inside a comparator re-derived it O(n log n) times and dominated a
    // warm `ripr pilot` on a 900-seam crate (#5348). Compute it once per
    // seam; `sort_by_cached_key` is stable, like the `sort_by` it replaces.
    actionable.sort_by_cached_key(|(change, entry)| (*change, RankKey::of(entry)));
    spread_across_owners(&mut actionable);
    actionable.truncate(max_seams);
    actionable.into_iter().map(|(_, entry)| entry).collect()
}

/// Actionable seams on lines the current change touches.
pub(super) fn actionable_in_change(
    classified: &[ClassifiedSeam],
    current_change: &PilotCurrentChange,
) -> usize {
    classified
        .iter()
        .filter(|entry| class_rank(entry.class).is_some() && current_change.touches(entry))
        .count()
}

/// Within each actionable class, rank every function's first seam ahead of
/// any function's second (#5770). Adjacent seams of one owner share its
/// tests, so when those tests are good every one of them is wrong together;
/// ranked by location alone they filled the top ten (8 of semver's 10 sat in
/// two `Identifier` methods). The class still leads, so a weak seam is never
/// pushed below an unrevealed one, and `RankKey` order holds inside each round.
///
/// Rounds count across classes on purpose: a function already listed for a
/// weak seam does not get a fresh first pick among the unrevealed ones, so
/// one function cannot claim a slot per class.
///
/// The current-change bucket still leads (#5480): the key is
/// `(in_change, class, round)`, so spreading never lifts an unchanged seam
/// above a changed one.
fn spread_across_owners<C: Ord + Copy>(ranked: &mut Vec<(C, &ClassifiedSeam)>) {
    let mut taken: BTreeMap<(&Path, &str), usize> = BTreeMap::new();
    let mut keyed = ranked
        .drain(..)
        .map(|(change, entry)| {
            let round = taken
                .entry((entry.seam.file(), entry.seam.owner()))
                .or_default();
            let key = (change, class_rank(entry.class), *round);
            *round += 1;
            (key, entry)
        })
        .collect::<Vec<_>>();
    // Stable, so seams with the same bucket, class and round keep `RankKey`
    // order.
    keyed.sort_by_key(|(key, _)| *key);
    ranked.extend(
        keyed
            .into_iter()
            .map(|((change, _, _), entry)| (change, entry)),
    );
}

/// Actionable seams that share an owning function with `entry`, itself
/// included, so a renderer can say how many more a ranked seam stands for.
/// Pass the same slice `top_actionable_seams` ranked: the renderer subtracts
/// the listed seams from this count, so a narrower slice would undercount.
pub(super) fn actionable_in_owner(classified: &[ClassifiedSeam], entry: &ClassifiedSeam) -> usize {
    classified
        .iter()
        .filter(|other| {
            class_rank(other.class).is_some()
                && other.seam.file() == entry.seam.file()
                && other.seam.owner() == entry.seam.owner()
        })
        .count()
}

/// The pilot ranking order, ascending: actionable class first, then seams
/// with missing discriminators, with related tests, and with a suggested
/// assertion, then file, line, kind and id as stable tie-breakers.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct RankKey<'a> {
    class: Option<u8>,
    missing_discriminators: u8,
    related_tests: u8,
    suggested_assertion: u8,
    file: String,
    line: usize,
    kind: &'a str,
    id: &'a str,
}

impl<'a> RankKey<'a> {
    fn of(entry: &'a ClassifiedSeam) -> Self {
        Self {
            class: class_rank(entry.class),
            missing_discriminators: bool_rank(!entry.evidence.missing_discriminators.is_empty()),
            related_tests: bool_rank(!entry.evidence.related_tests.is_empty()),
            suggested_assertion: bool_rank(
                suggested_assertion_for_classified_seam(entry).is_some(),
            ),
            file: display_path(entry.seam.file()),
            line: entry.seam.display_line(),
            kind: entry.seam.kind().as_str(),
            id: entry.seam.id().as_str(),
        }
    }
}

/// Seams withheld from the ranking because their class names a static
/// limitation, not a gap. Renderers disclose this count so a short or empty
/// ranking is never read as a clean result (#5497).
pub(super) fn withheld_static_limitations(classified: &[ClassifiedSeam]) -> usize {
    classified
        .iter()
        .filter(|entry| entry.class.is_static_limitation())
        .count()
}

pub(super) fn actionable_total(classified: &[ClassifiedSeam]) -> usize {
    classified
        .iter()
        .filter(|entry| class_rank(entry.class).is_some())
        .count()
}

/// Whether pilot can recommend this seam. The seam budget uses the same
/// predicate, so a changed seam kept past the cut is one ranking can use.
pub(super) fn is_actionable(entry: &ClassifiedSeam) -> bool {
    class_rank(entry.class).is_some()
}

/// Pilot ranks gap classes only. A class that names a static limitation
/// (`SeamGripClass::is_static_limitation`: `opaque` and the `*_unknown`
/// classes) is withheld from the ranking and counted by
/// [`withheld_static_limitations`] instead, so pilot never offers a seam
/// whose evidence it could not establish as the gap to test first (#5497).
fn class_rank(class: SeamGripClass) -> Option<u8> {
    if class.is_static_limitation() {
        return None;
    }
    Some(match class {
        SeamGripClass::WeaklyGripped => 0,
        SeamGripClass::Ungripped => 1,
        SeamGripClass::ReachableUnrevealed => 2,
        SeamGripClass::ActivationUnknown
        | SeamGripClass::PropagationUnknown
        | SeamGripClass::ObservationUnknown
        | SeamGripClass::DiscriminationUnknown
        | SeamGripClass::Opaque
        | SeamGripClass::StronglyGripped
        | SeamGripClass::Intentional
        | SeamGripClass::Suppressed => return None,
    })
}

fn bool_rank(value: bool) -> u8 {
    if value { 0 } else { 1 }
}

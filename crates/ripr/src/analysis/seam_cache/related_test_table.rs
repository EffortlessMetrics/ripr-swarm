//! Classified-seam cache body with each distinct related test written once.
//!
//! Full evidence repeats the same related-test records across many seams: on
//! regex, 10,000 seams carry 438,134 related-test entries but only 1,155
//! distinct records, and those repeats were 94% of a 429 MB cache file. The
//! body now stores a `related_tests` table of distinct records and gives each
//! seam the table indices of its tests, in its original order.
//!
//! Decoding hands every seam that names a row the table's one shared record,
//! so a warm run holds each distinct test once in memory as well (#5341).
//!
//! Records are deduplicated by their own serialized JSON, so a decoded seam's
//! related tests serialize exactly as the stored ones did. Field lists are
//! destructured exhaustively: a new `ClassifiedSeam` or `TestGripEvidence`
//! field fails to compile here until the cache body carries it.

use crate::analysis::new_test_target::NewTestTargetAdmission;
use crate::analysis::seam_classification::ClassifiedSeam;
use crate::analysis::seams::{RepoSeam, SeamGripClass, SeamId};
use crate::analysis::test_grip_evidence::{RelatedTestGrip, TestGripEvidence};
use crate::domain::{MissingDiscriminatorFact, StageEvidence, ValueFact};
use serde::de::Error as _;
use serde::ser::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Serialize)]
struct TableRef<'a> {
    related_tests: Vec<&'a RelatedTestGrip>,
    seams: Vec<SeamRef<'a>>,
}

#[derive(Serialize)]
struct SeamRef<'a> {
    seam: &'a RepoSeam,
    evidence: EvidenceRef<'a>,
    class: &'a SeamGripClass,
}

#[derive(Serialize)]
struct EvidenceRef<'a> {
    seam_id: &'a SeamId,
    related_tests: Vec<u32>,
    reach: &'a StageEvidence,
    activate: &'a StageEvidence,
    propagate: &'a StageEvidence,
    observe: &'a StageEvidence,
    discriminate: &'a StageEvidence,
    observed_values: &'a [ValueFact],
    missing_discriminators: &'a [MissingDiscriminatorFact],
    #[serde(skip_serializing_if = "Option::is_none")]
    new_test_target: Option<&'a NewTestTargetAdmission>,
}

#[derive(Deserialize)]
struct TableOwned {
    related_tests: Vec<Arc<RelatedTestGrip>>,
    seams: Vec<SeamOwned>,
}

#[derive(Deserialize)]
struct SeamOwned {
    seam: RepoSeam,
    evidence: EvidenceOwned,
    class: SeamGripClass,
}

#[derive(Deserialize)]
struct EvidenceOwned {
    seam_id: SeamId,
    related_tests: Vec<u32>,
    reach: StageEvidence,
    activate: StageEvidence,
    propagate: StageEvidence,
    observe: StageEvidence,
    discriminate: StageEvidence,
    observed_values: Vec<ValueFact>,
    missing_discriminators: Vec<MissingDiscriminatorFact>,
    #[serde(default)]
    new_test_target: Option<NewTestTargetAdmission>,
}

pub(super) fn serialize<S: Serializer>(
    seams: &[ClassifiedSeam],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let mut related_tests: Vec<&RelatedTestGrip> = Vec::new();
    let mut index_of: HashMap<Vec<u8>, u32> = HashMap::new();
    let mut seam_refs = Vec::with_capacity(seams.len());
    for classified in seams {
        let seam_tests = &classified.evidence.related_tests;
        let mut indices = Vec::with_capacity(seam_tests.len());
        for test in seam_tests {
            let key = serde_json::to_vec(test).map_err(S::Error::custom)?;
            let next = u32::try_from(related_tests.len()).map_err(S::Error::custom)?;
            let index = *index_of.entry(key).or_insert_with(|| {
                related_tests.push(test);
                next
            });
            indices.push(index);
        }
        seam_refs.push(seam_ref(classified, indices));
    }
    TableRef {
        related_tests,
        seams: seam_refs,
    }
    .serialize(serializer)
}

fn seam_ref(classified: &ClassifiedSeam, related_tests: Vec<u32>) -> SeamRef<'_> {
    let ClassifiedSeam {
        seam,
        evidence,
        class,
    } = classified;
    let TestGripEvidence {
        seam_id,
        related_tests: _,
        reach,
        activate,
        propagate,
        observe,
        discriminate,
        observed_values,
        missing_discriminators,
        statically_contradicted_related_tests: _,
        new_test_target,
    } = evidence;
    SeamRef {
        seam,
        evidence: EvidenceRef {
            seam_id,
            related_tests,
            reach,
            activate,
            propagate,
            observe,
            discriminate,
            observed_values,
            missing_discriminators,
            new_test_target: new_test_target.as_ref(),
        },
        class,
    }
}

/// Pretty-printed size of a table body as seams are appended, for shard
/// planning (#5364).
///
/// The planner used to find each shard's end by re-encoding growing prefixes,
/// about a dozen full encodes of the payload. This sizer encodes each seam and
/// each newly tabled test once. It mirrors [`serialize`] under
/// `serde_json::to_writer_pretty` with the body nested two levels deep, as
/// in a cache envelope: both arrays sit at depth 2 and their items at depth
/// 3, so an item adds six bytes of indentation per line break to its
/// standalone pretty size. Callers confirm the result against a real encode.
#[derive(Default)]
pub(super) struct PrettyTableSizer {
    index_of: HashMap<Vec<u8>, u32>,
    tests: usize,
    test_bytes: usize,
    seams: usize,
    seam_bytes: usize,
}

/// A seam measured against a [`PrettyTableSizer`] but not yet added to it.
pub(super) struct MeasuredSeam {
    new_tests: Vec<Vec<u8>>,
    new_test_bytes: usize,
    seam_bytes: usize,
}

const PRETTY_ITEM_INDENT_PER_LINE: usize = 6;

impl PrettyTableSizer {
    /// Bytes the body adds to an envelope whose body is the empty table
    /// (`{"related_tests": [], "seams": []}`).
    pub(super) fn bytes_over_empty(&self) -> usize {
        pretty_array_growth(self.tests, self.test_bytes)
            .saturating_add(pretty_array_growth(self.seams, self.seam_bytes))
    }

    /// [`Self::bytes_over_empty`] once `measured` is added.
    pub(super) fn bytes_over_empty_with(&self, measured: &MeasuredSeam) -> usize {
        pretty_array_growth(
            self.tests.saturating_add(measured.new_tests.len()),
            self.test_bytes.saturating_add(measured.new_test_bytes),
        )
        .saturating_add(pretty_array_growth(
            self.seams.saturating_add(1),
            self.seam_bytes.saturating_add(measured.seam_bytes),
        ))
    }

    pub(super) fn measure(&self, classified: &ClassifiedSeam) -> Result<MeasuredSeam, String> {
        let seam_tests = &classified.evidence.related_tests;
        let mut new_tests: Vec<Vec<u8>> = Vec::new();
        let mut pending: HashMap<Vec<u8>, usize> = HashMap::new();
        let mut new_test_bytes = 0usize;
        let mut indices = Vec::with_capacity(seam_tests.len());
        for test in seam_tests {
            let key = serde_json::to_vec(test).map_err(|err| err.to_string())?;
            let index = match self.index_of.get(&key) {
                Some(index) => *index,
                None => {
                    let offset = match pending.get(&key) {
                        Some(offset) => *offset,
                        None => {
                            new_test_bytes =
                                new_test_bytes.saturating_add(pretty_item_bytes(test)?);
                            let offset = new_tests.len();
                            pending.insert(key.clone(), offset);
                            new_tests.push(key);
                            offset
                        }
                    };
                    u32::try_from(self.tests.saturating_add(offset))
                        .map_err(|err| err.to_string())?
                }
            };
            indices.push(index);
        }
        let seam_bytes = pretty_item_bytes(&seam_ref(classified, indices))?;
        Ok(MeasuredSeam {
            new_tests,
            new_test_bytes,
            seam_bytes,
        })
    }

    pub(super) fn add(&mut self, measured: MeasuredSeam) -> Result<(), String> {
        for key in measured.new_tests {
            let index = u32::try_from(self.tests).map_err(|err| err.to_string())?;
            self.index_of.insert(key, index);
            self.tests = self.tests.saturating_add(1);
        }
        self.test_bytes = self.test_bytes.saturating_add(measured.new_test_bytes);
        self.seams = self.seams.saturating_add(1);
        self.seam_bytes = self.seam_bytes.saturating_add(measured.seam_bytes);
        Ok(())
    }
}

/// A pretty array at depth 2 is `[]` when empty, and otherwise `[`, then each
/// item on its own line after six spaces, joined by `,`, then a line holding
/// four spaces and `]`: items plus 8 bytes each plus 6.
fn pretty_array_growth(items: usize, item_bytes: usize) -> usize {
    if items == 0 {
        0
    } else {
        item_bytes
            .saturating_add(items.saturating_mul(8))
            .saturating_add(6)
            .saturating_sub(2)
    }
}

fn pretty_item_bytes<T: Serialize>(value: &T) -> Result<usize, String> {
    struct LineCounter {
        bytes: usize,
        newlines: usize,
    }
    impl std::io::Write for LineCounter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.bytes = self.bytes.saturating_add(buf.len());
            self.newlines = self
                .newlines
                .saturating_add(buf.iter().filter(|&&byte| byte == b'\n').count());
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut counter = LineCounter {
        bytes: 0,
        newlines: 0,
    };
    serde_json::to_writer_pretty(&mut counter, value).map_err(|err| err.to_string())?;
    Ok(counter
        .bytes
        .saturating_add(counter.newlines.saturating_mul(PRETTY_ITEM_INDENT_PER_LINE)))
}

pub(super) fn deserialize<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<ClassifiedSeam>, D::Error> {
    let TableOwned {
        related_tests: table,
        seams,
    } = TableOwned::deserialize(deserializer)?;
    let mut classified = Vec::with_capacity(seams.len());
    for SeamOwned {
        seam,
        evidence,
        class,
    } in seams
    {
        let EvidenceOwned {
            seam_id,
            related_tests: indices,
            reach,
            activate,
            propagate,
            observe,
            discriminate,
            observed_values,
            missing_discriminators,
            new_test_target,
        } = evidence;
        let mut related_tests = Vec::with_capacity(indices.len());
        for index in indices {
            let test = usize::try_from(index)
                .ok()
                .and_then(|index| table.get(index))
                .ok_or_else(|| {
                    D::Error::custom(format!(
                        "related test index {index} outside a table of {}",
                        table.len()
                    ))
                })?;
            related_tests.push(Arc::clone(test));
        }
        classified.push(ClassifiedSeam {
            seam,
            evidence: TestGripEvidence {
                seam_id,
                related_tests,
                reach,
                activate,
                propagate,
                observe,
                discriminate,
                observed_values,
                missing_discriminators,
                statically_contradicted_related_tests: 0,
                new_test_target,
            },
            class,
        });
    }
    Ok(classified)
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::analysis::seams::{ExpectedSink, RequiredDiscriminator, SeamKind};
    use crate::analysis::test_grip_evidence::{TestKind, TestTargetEvidence};
    use crate::domain::{
        Confidence, OracleKind, OracleStrength, RelationConfidence, RelationReason, StageState,
        SymbolId,
    };
    use std::path::PathBuf;

    #[derive(Serialize, Deserialize)]
    struct Body {
        #[serde(with = "super")]
        seams: Vec<ClassifiedSeam>,
    }

    pub(in crate::analysis::seam_cache) fn related(
        name: &str,
        reason: RelationReason,
    ) -> RelatedTestGrip {
        RelatedTestGrip {
            test_name: name.to_owned(),
            file: PathBuf::from("tests/it.rs"),
            line: 7,
            test_target: Some(TestTargetEvidence::from_index(
                SymbolId(format!("tests/it.rs::{name}")),
                PathBuf::from("tests/it.rs"),
                7,
                TestKind::Integration,
                reason,
                "sha256:workspace".to_owned(),
            )),
            oracle_kind: OracleKind::ExactValue,
            oracle_strength: OracleStrength::Strong,
            evidence_summary: "exact value assertion".to_owned(),
            relation_reason: reason,
            relation_confidence: RelationConfidence::Medium,
        }
    }

    pub(in crate::analysis::seam_cache) fn seam(
        line: usize,
        related_tests: Vec<RelatedTestGrip>,
    ) -> ClassifiedSeam {
        let seam = RepoSeam::new(
            PathBuf::from("src/foo.rs"),
            "src/foo.rs::foo",
            SeamKind::PredicateBoundary,
            line * 10,
            line,
            "x > 5".to_owned(),
            RequiredDiscriminator::BoundaryValue {
                description: "x > 5".to_owned(),
            },
            ExpectedSink::ReturnValue,
        );
        let evidence = TestGripEvidence {
            seam_id: seam.id().clone(),
            related_tests: related_tests.into_iter().map(Arc::new).collect(),
            reach: StageEvidence::new(StageState::Yes, Confidence::High, "reach"),
            activate: StageEvidence::new(StageState::Unknown, Confidence::Medium, "activate"),
            propagate: StageEvidence::new(StageState::Unknown, Confidence::Medium, "propagate"),
            observe: StageEvidence::new(StageState::Weak, Confidence::Low, "observe"),
            discriminate: StageEvidence::new(StageState::No, Confidence::Low, "discriminate"),
            observed_values: Vec::new(),
            missing_discriminators: Vec::new(),
            statically_contradicted_related_tests: 0,
            new_test_target: None,
        };
        ClassifiedSeam {
            seam,
            evidence,
            class: SeamGripClass::Ungripped,
        }
    }

    #[test]
    fn table_round_trip_keeps_every_seam_and_shares_repeated_tests() -> Result<(), String> {
        let a = related("a", RelationReason::SameModule);
        let b = related("b", RelationReason::SameModule);
        // Same test under another relation is a distinct record, not a repeat.
        let a_direct = related("a", RelationReason::DirectOwnerCall);
        let seams = vec![
            seam(1, vec![a.clone(), b.clone()]),
            seam(2, vec![b.clone(), a.clone(), a_direct]),
            seam(3, Vec::new()),
            seam(4, vec![a]),
        ];
        let encoded = serde_json::to_value(Body {
            seams: seams.clone(),
        })
        .map_err(|err| err.to_string())?;
        let table = encoded["seams"]["related_tests"]
            .as_array()
            .ok_or("missing related test table")?;
        assert_eq!(table.len(), 3);
        assert_eq!(
            encoded["seams"]["seams"][1]["evidence"]["related_tests"],
            serde_json::json!([1, 0, 2])
        );
        let decoded: Body = serde_json::from_value(encoded).map_err(|err| err.to_string())?;
        assert_eq!(
            serde_json::to_value(&decoded.seams).map_err(|err| err.to_string())?,
            serde_json::to_value(&seams).map_err(|err| err.to_string())?
        );
        // A warm load hands every seam the table's one record, so a decoded
        // cache holds each distinct test once (#5341).
        let tests = |position: usize| &decoded.seams[position].evidence.related_tests;
        assert!(Arc::ptr_eq(&tests(0)[0], &tests(1)[1]));
        assert!(Arc::ptr_eq(&tests(0)[0], &tests(3)[0]));
        assert!(Arc::ptr_eq(&tests(0)[1], &tests(1)[0]));
        assert!(!Arc::ptr_eq(&tests(0)[0], &tests(1)[2]));
        Ok(())
    }

    #[test]
    fn duplicate_occurrences_in_one_seam_survive_decode() -> Result<(), String> {
        // Sharing reuses the backing record but never deduplicates a seam's
        // occurrence list (#5341).
        let a = related("a", RelationReason::SameModule);
        let b = related("b", RelationReason::SameModule);
        let seams = vec![seam(1, vec![a.clone(), b, a])];
        let encoded = serde_json::to_value(Body {
            seams: seams.clone(),
        })
        .map_err(|err| err.to_string())?;
        assert_eq!(
            encoded["seams"]["seams"][0]["evidence"]["related_tests"],
            serde_json::json!([0, 1, 0])
        );
        let decoded: Body = serde_json::from_value(encoded).map_err(|err| err.to_string())?;
        assert_eq!(
            serde_json::to_value(&decoded.seams).map_err(|err| err.to_string())?,
            serde_json::to_value(&seams).map_err(|err| err.to_string())?
        );
        let tests = &decoded.seams[0].evidence.related_tests;
        assert_eq!(tests.len(), 3);
        assert!(Arc::ptr_eq(&tests[0], &tests[2]));
        assert!(!Arc::ptr_eq(&tests[0], &tests[1]));
        Ok(())
    }

    #[test]
    fn high_fan_out_table_decodes_to_one_record_per_row() -> Result<(), String> {
        // 2,000 seams naming the same three tests decode to three records,
        // and a forged run of repeated indices costs one pointer per index,
        // not one record (#5341, #5124).
        let names = ["a", "b", "c"];
        let seams: Vec<ClassifiedSeam> = (1..=2_000)
            .map(|line| {
                seam(
                    line,
                    names
                        .iter()
                        .map(|name| related(name, RelationReason::SameModule))
                        .collect(),
                )
            })
            .collect();
        let mut encoded = serde_json::to_value(Body { seams }).map_err(|err| err.to_string())?;
        assert_eq!(
            encoded["seams"]["related_tests"]
                .as_array()
                .ok_or("missing related test table")?
                .len(),
            names.len()
        );
        encoded["seams"]["seams"][0]["evidence"]["related_tests"] =
            serde_json::json!(vec![0; 100_000]);
        let decoded: Body = serde_json::from_value(encoded).map_err(|err| err.to_string())?;
        let occurrences: usize = decoded
            .seams
            .iter()
            .map(|classified| classified.evidence.related_tests.len())
            .sum();
        let records: std::collections::HashSet<*const RelatedTestGrip> = decoded
            .seams
            .iter()
            .flat_map(|classified| classified.evidence.related_tests.iter())
            .map(Arc::as_ptr)
            .collect();
        assert_eq!(occurrences, 100_000 + 1_999 * names.len());
        assert_eq!(records.len(), names.len());
        Ok(())
    }

    #[test]
    fn index_outside_the_table_is_a_decode_error() -> Result<(), String> {
        let mut encoded = serde_json::to_value(Body {
            seams: vec![seam(1, vec![related("a", RelationReason::SameModule)])],
        })
        .map_err(|err| err.to_string())?;
        encoded["seams"]["seams"][0]["evidence"]["related_tests"] = serde_json::json!([1]);
        let Err(err) = serde_json::from_value::<Body>(encoded) else {
            return Err("an out-of-range index must not decode".to_owned());
        };
        assert!(
            err.to_string()
                .contains("related test index 1 outside a table of 1")
        );
        Ok(())
    }
}

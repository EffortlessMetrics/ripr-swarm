//! Inactive Windows-subset admission seam for #4987.
//!
//! The trusted controller supplies observations and atomically persists a claim
//! before launching a product worker. This pure decision neither authenticates
//! caller data nor acquires custody. Ready and ordinary PR events are not requests.

use crate::convergence::types::{CommitObservation, EvidenceState, ObjectId, ReviewObservation};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Request {
    pub request_id: String,
    pub repository: String,
    pub target: String,
    pub pr_number: u64,
    pub head: ObjectId,
    pub base: ObjectId,
    pub subject: CommitObservation,
    pub controller_generation: ObjectId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Current {
    pub repository: String,
    pub target: String,
    pub pr_number: u64,
    pub open: bool,
    pub draft: bool,
    pub trusted_product_execution: bool,
    pub head: ObjectId,
    pub base: ObjectId,
    pub subject: CommitObservation,
    pub controller_generation: ObjectId,
    pub review: ReviewObservation,
    pub proof: EvidenceState,
    pub complete_selection: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestState {
    Claimed,
    Queued,
    Running,
    Passed,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExistingRequest {
    pub request: Request,
    pub producer_run: u64,
    pub producer_attempt: u64,
    pub state: RequestState,
}

/// Unreadable lookup differs from a verified absence of a prior request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CustodyObservation {
    Unavailable,
    Absent,
    Existing(Box<ExistingRequest>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Refusal {
    MissingRequest,
    MalformedIdentity,
    WrongRepositoryOrTarget,
    IneligiblePullRequest,
    UntrustedExecution,
    StaleHead,
    StaleBase,
    WrongIntegration,
    StaleGeneration,
    MissingReview,
    MissingProof,
    IncompleteSelection,
    CustodyUnavailable,
    ConflictingCustody,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Decision {
    Refuse(Refusal),
    /// A proposal to persist one atomic claim, never permission to launch yet.
    Claim,
    /// Existing failure is returned too: duplicate delivery cannot retry it.
    Reuse {
        producer_run: u64,
        producer_attempt: u64,
        state: RequestState,
    },
}

fn object(id: &ObjectId) -> bool {
    id.0.len() == 40
        && id
            .0
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn request_id(id: &str) -> bool {
    let Some((run, attempt)) = id.split_once('-') else {
        return false;
    };
    [run, attempt].iter().all(|part| {
        part.parse::<u64>().is_ok_and(|value| value > 0) && part.bytes().all(|b| b.is_ascii_digit())
    })
}

fn same_subject(left: &Request, right: &Request) -> bool {
    left.repository == right.repository
        && left.target == right.target
        && left.pr_number == right.pr_number
        && left.head == right.head
        && left.base == right.base
        && left.subject == right.subject
        && left.controller_generation == right.controller_generation
}

pub fn decide(
    request: Option<&Request>,
    current: &Current,
    custody: &CustodyObservation,
) -> Decision {
    use Refusal as R;
    let Some(request) = request else {
        return Decision::Refuse(R::MissingRequest);
    };
    if !request_id(&request.request_id)
        || request.pr_number == 0
        || [
            &request.head,
            &request.base,
            &request.subject.commit,
            &request.subject.tree,
            &request.controller_generation,
        ]
        .iter()
        .any(|id| !object(id))
    {
        return Decision::Refuse(R::MalformedIdentity);
    }
    if request.repository != current.repository
        || request.target != current.target
        || request.pr_number != current.pr_number
        || current.repository != "EffortlessMetrics/ripr-swarm"
        || current.target != "main"
    {
        return Decision::Refuse(R::WrongRepositoryOrTarget);
    }
    if !current.open || current.draft {
        return Decision::Refuse(R::IneligiblePullRequest);
    }
    if !current.trusted_product_execution {
        return Decision::Refuse(R::UntrustedExecution);
    }
    if request.head != current.head {
        return Decision::Refuse(R::StaleHead);
    }
    if request.base != current.base {
        return Decision::Refuse(R::StaleBase);
    }
    if request.subject != current.subject
        || request.subject.parents != [request.base.clone(), request.head.clone()]
    {
        return Decision::Refuse(R::WrongIntegration);
    }
    if request.controller_generation != current.controller_generation {
        return Decision::Refuse(R::StaleGeneration);
    }
    if current.review.subject != request.head
        || current.review.state != EvidenceState::Passed
        || current.review.unresolved_threads != 0
    {
        return Decision::Refuse(R::MissingReview);
    }
    if current.proof != EvidenceState::Passed {
        return Decision::Refuse(R::MissingProof);
    }
    if !current.complete_selection {
        return Decision::Refuse(R::IncompleteSelection);
    }
    match custody {
        CustodyObservation::Unavailable => Decision::Refuse(R::CustodyUnavailable),
        CustodyObservation::Absent => Decision::Claim,
        CustodyObservation::Existing(existing)
            if same_subject(request, &existing.request)
                && request_id(&existing.request.request_id)
                && existing.producer_run > 0
                && existing.producer_attempt > 0 =>
        {
            Decision::Reuse {
                producer_run: existing.producer_run,
                producer_attempt: existing.producer_attempt,
                state: existing.state,
            }
        }
        CustodyObservation::Existing(_) => Decision::Refuse(R::ConflictingCustody),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(digit: char) -> ObjectId {
        ObjectId(digit.to_string().repeat(40))
    }

    fn fixture() -> (Request, Current) {
        let subject = CommitObservation {
            commit: id('c'),
            tree: id('d'),
            parents: vec![id('b'), id('a')],
        };
        let request = Request {
            request_id: "123-1".into(),
            repository: "EffortlessMetrics/ripr-swarm".into(),
            target: "main".into(),
            pr_number: 8008,
            head: id('a'),
            base: id('b'),
            subject: subject.clone(),
            controller_generation: id('e'),
        };
        let current = Current {
            repository: request.repository.clone(),
            target: request.target.clone(),
            pr_number: request.pr_number,
            open: true,
            draft: false,
            trusted_product_execution: true,
            head: request.head.clone(),
            base: request.base.clone(),
            subject,
            controller_generation: request.controller_generation.clone(),
            review: ReviewObservation {
                subject: id('a'),
                unresolved_threads: 0,
                state: EvidenceState::Passed,
            },
            proof: EvidenceState::Passed,
            complete_selection: true,
        };
        (request, current)
    }

    #[test]
    fn explicit_current_request_proposes_a_claim_not_a_launch_or_success() {
        let (request, current) = fixture();
        assert_eq!(
            decide(Some(&request), &current, &CustodyObservation::Absent),
            Decision::Claim
        );
    }

    #[test]
    fn ready_push_and_draft_without_request_cannot_admit() {
        let (_, mut current) = fixture();
        for draft in [true, false] {
            current.draft = draft;
            assert_eq!(
                decide(None, &current, &CustodyObservation::Absent),
                Decision::Refuse(Refusal::MissingRequest)
            );
        }
    }

    #[test]
    fn unreadable_custody_is_not_verified_absence() {
        let (request, current) = fixture();
        assert_eq!(
            decide(Some(&request), &current, &CustodyObservation::Unavailable),
            Decision::Refuse(Refusal::CustodyUnavailable)
        );
    }

    #[test]
    fn stale_head_base_generation_and_cleanly_mergeable_wrong_tree_refuse() {
        let (request, current) = fixture();
        for (field, expected) in [
            ("head", Refusal::StaleHead),
            ("base", Refusal::StaleBase),
            ("generation", Refusal::StaleGeneration),
            ("tree", Refusal::WrongIntegration),
        ] {
            let mut changed = current.clone();
            match field {
                "head" => changed.head = id('f'),
                "base" => changed.base = id('f'),
                "generation" => changed.controller_generation = id('f'),
                _ => changed.subject.tree = id('f'),
            }
            assert_eq!(
                decide(Some(&request), &changed, &CustodyObservation::Absent),
                Decision::Refuse(expected)
            );
        }
    }

    #[test]
    fn review_proof_selection_draft_and_trust_are_distinct_refusals() {
        let (request, current) = fixture();
        for (field, expected) in [
            ("review", Refusal::MissingReview),
            ("proof", Refusal::MissingProof),
            ("selection", Refusal::IncompleteSelection),
            ("draft", Refusal::IneligiblePullRequest),
            ("trust", Refusal::UntrustedExecution),
            ("findings", Refusal::MissingReview),
        ] {
            let mut changed = current.clone();
            match field {
                "review" => changed.review.state = EvidenceState::NotProven,
                "proof" => changed.proof = EvidenceState::Failed,
                "selection" => changed.complete_selection = false,
                "draft" => changed.draft = true,
                "trust" => changed.trusted_product_execution = false,
                _ => changed.review.unresolved_threads = 1,
            }
            assert_eq!(
                decide(Some(&request), &changed, &CustodyObservation::Absent),
                Decision::Refuse(expected)
            );
        }
    }

    #[test]
    fn duplicate_and_lost_response_reuse_every_existing_state_including_failure() {
        let (mut request, current) = fixture();
        let original = request.clone();
        request.request_id = "124-1".into();
        for state in [
            RequestState::Claimed,
            RequestState::Queued,
            RequestState::Running,
            RequestState::Passed,
            RequestState::Failed,
        ] {
            let prior = CustodyObservation::Existing(Box::new(ExistingRequest {
                request: original.clone(),
                producer_run: 456,
                producer_attempt: 2,
                state,
            }));
            assert_eq!(
                decide(Some(&request), &current, &prior),
                Decision::Reuse {
                    producer_run: 456,
                    producer_attempt: 2,
                    state
                }
            );
        }
    }

    #[test]
    fn mismatched_or_missing_persisted_producer_never_reuses_or_reallocates() {
        let (request, current) = fixture();
        for stale in [true, false] {
            let mut prior = ExistingRequest {
                request: request.clone(),
                producer_run: 456,
                producer_attempt: 2,
                state: RequestState::Running,
            };
            if stale {
                prior.request.subject.tree = id('f');
            } else {
                prior.producer_run = 0;
            }
            assert_eq!(
                decide(
                    Some(&request),
                    &current,
                    &CustodyObservation::Existing(Box::new(prior))
                ),
                Decision::Refuse(Refusal::ConflictingCustody)
            );
        }
    }

    #[test]
    fn malformed_request_and_head_only_subject_refuse() {
        let (request, current) = fixture();
        for input in ["", "0-1", "1-0", "1-2-3", "run-1", "1-+1"] {
            let mut bad = request.clone();
            bad.request_id = input.into();
            assert_eq!(
                decide(Some(&bad), &current, &CustodyObservation::Absent),
                Decision::Refuse(Refusal::MalformedIdentity)
            );
        }
        let mut bad = request;
        bad.subject.parents.clear();
        let mut observed = current;
        observed.subject = bad.subject.clone();
        assert_eq!(
            decide(Some(&bad), &observed, &CustodyObservation::Absent),
            Decision::Refuse(Refusal::WrongIntegration)
        );
    }

    #[test]
    fn old_running_custody_cannot_admit_a_changed_head() {
        let (request, mut current) = fixture();
        let prior = CustodyObservation::Existing(Box::new(ExistingRequest {
            request: request.clone(),
            producer_run: 456,
            producer_attempt: 2,
            state: RequestState::Running,
        }));
        current.head = id('f');
        assert_eq!(
            decide(Some(&request), &current, &prior),
            Decision::Refuse(Refusal::StaleHead)
        );
    }

    #[test]
    fn wrong_repository_target_pr_and_closed_state_refuse() {
        let (request, current) = fixture();
        for field in ["repository", "target", "pr", "closed"] {
            let mut changed = current.clone();
            let expected = match field {
                "repository" => {
                    changed.repository = "fork/repository".into();
                    Refusal::WrongRepositoryOrTarget
                }
                "target" => {
                    changed.target = "release".into();
                    Refusal::WrongRepositoryOrTarget
                }
                "pr" => {
                    changed.pr_number += 1;
                    Refusal::WrongRepositoryOrTarget
                }
                _ => {
                    changed.open = false;
                    Refusal::IneligiblePullRequest
                }
            };
            assert_eq!(
                decide(Some(&request), &changed, &CustodyObservation::Absent),
                Decision::Refuse(expected)
            );
        }
    }

    #[test]
    fn malformed_object_and_reversed_parents_cannot_be_current_proof() {
        let (request, current) = fixture();
        let mut malformed = request.clone();
        malformed.head = ObjectId("a".repeat(39));
        assert_eq!(
            decide(Some(&malformed), &current, &CustodyObservation::Absent),
            Decision::Refuse(Refusal::MalformedIdentity)
        );
        let mut reversed = request;
        reversed.subject.parents.reverse();
        let mut changed = current;
        changed.subject = reversed.subject.clone();
        assert_eq!(
            decide(Some(&reversed), &changed, &CustodyObservation::Absent),
            Decision::Refuse(Refusal::WrongIntegration)
        );
    }
}

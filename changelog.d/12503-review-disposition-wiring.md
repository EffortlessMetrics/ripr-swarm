<!-- section: Internal / CI -->
- Wire the review-disposition verifier into `routed-rust.yml`: a `merge-ready` request is refused at request time without a current-head source review attestation, and the Result job re-reads it from the immutable trusted bundle before the eligibility recheck (#8157, #12503).

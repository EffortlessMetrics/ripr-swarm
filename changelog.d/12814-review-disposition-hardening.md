<!-- section: Internal / CI -->
- Harden the review-disposition verifier before it is wired into admission: a trusted reviewer's negative comment refuses despite another reviewer's ready comment, negating prose after the marker refuses, a decisive review without `submitted_at` refuses, and malformed review pages refuse with a refusal line (#8157, #12814).

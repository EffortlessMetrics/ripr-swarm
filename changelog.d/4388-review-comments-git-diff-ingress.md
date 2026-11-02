<!-- section: Fixed -->
- `ripr review-comments` now captures Git range diff stdout under
  `RIPR_REVIEW_GUIDANCE_MAX_PAYLOAD_BYTES` while the child runs, refusing with
  `review_guidance_oversized` before unbounded ingress allocation (#4388,
  #5000).

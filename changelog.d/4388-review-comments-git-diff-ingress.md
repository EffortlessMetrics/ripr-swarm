<!-- section: Fixed -->
- `ripr review-comments` now captures Git range diff stdout under
  `RIPR_REVIEW_GUIDANCE_MAX_PAYLOAD_BYTES` while the child runs, refusing with
  `review_guidance_oversized` before unbounded ingress allocation (#4388,
  #5000).
- Git diff stdout decoding for analysis loaders moves valid UTF-8 bytes into
  the owned diff text without a second complete payload buffer on the accepted
  path (#5000).

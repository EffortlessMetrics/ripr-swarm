<!-- section: Internal / CI -->
- `.github/workflows/routed-rust.yml` raises the per-candidate
  `max-changed-lines` admission cap from 3000 to 3700 in both the queue and
  qualification jobs. #14513 at e4b60f7107 is 3,603 counted lines, so 3,000
  refuses it by 603 and 3,700 admits it with 97 lines of headroom (#14513).

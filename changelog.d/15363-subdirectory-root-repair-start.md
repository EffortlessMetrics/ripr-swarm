<!-- section: Fixed -->
- `ripr agent repair` / `ripr repair` under a `--root` below the Git top-level now refuse with "not the exact Git top-level"
  before taking a lock or writing `target/ripr` under that root; the before phase used to create workflow files and an attempt lock
  there first (#15363).

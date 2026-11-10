<!-- section: Internal / CI -->
- Retire auxiliary PR/review/comment Actions and duplicate auxiliary main-push
  builds, replace the periodic main-health probe with explicit incident proof,
  retain full-commit historical evidence without automatic incident closure,
  and enforce the source event perimeter while retaining the protected caller
  and cache-writer bootstrap dependencies for coordinated queue cutover (#4842).
- The main-health replay harness creates its temporary directory before
  attaching cleanup, so a path collision leaves existing contents untouched
  (#4842).

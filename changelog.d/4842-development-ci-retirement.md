<!-- section: Internal / CI -->
- Retire auxiliary PR/review/comment Actions and duplicate auxiliary main-push
  builds, and enforce the source event perimeter while retaining the protected
  caller and cache-writer bootstrap dependencies for coordinated queue cutover
  (#4842).
- Retain the existing main-health sentinel unchanged, under an exact timer
  exception, until the dependent incident-probe redesign and its tests land
  together (#4842).

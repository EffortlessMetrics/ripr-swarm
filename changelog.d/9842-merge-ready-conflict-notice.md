<!-- section: Fixed -->
- A `merge-ready` request on a PR that conflicts with its base no longer
  vanishes. GitHub creates no `pull_request` run for a conflicting PR, so
  `routed-rust.yml` started nothing and the label gave no signal. A small
  `pull_request_target` workflow now comments once and posts a failing
  `merge-ready conflict` status naming the conflict. It checks out no PR code
  (#9842, #9250).

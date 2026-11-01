<!-- section: Internal / CI -->
- The routed Result now binds the extension e2e verdict to the `ci.yml` run that names its own request (`CI merge-ready request <head> <request time>`) instead of a creation-time window, so two `merge-ready` requests seconds apart can no longer read each other's suite (#12236).

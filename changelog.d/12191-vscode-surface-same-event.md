<!-- section: Internal / CI -->
- The routed Result now binds the extension e2e verdict to the `ci.yml` run created at the same label event as the routed run (within 30 seconds on either side, closest wins), so a second `merge-ready` on the same head no longer reads the first request's suite (#12191).

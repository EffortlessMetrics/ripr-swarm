<!-- section: Internal / CI -->
- The Windows isolated-target path test and the support-tier fast-route fixture match what the Windows host prints; the fixture uses a short private clone path so the real `cargo fmt --check` command line stays under Windows' 32,767-character limit (about 27.5k characters against 35.4k before) (#9272).

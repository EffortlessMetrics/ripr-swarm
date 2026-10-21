<!-- section: Internal / CI -->
- The workspace-root binding test harness matched folder containment with a hard-coded `/`, so two navigation and copy tests waited forever on Windows (found by the packaged Windows rehearsal). It now uses the platform separator (#2769).

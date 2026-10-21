<!-- section: Fixed -->
- The VS Code extension test runner counts only passed and failed tests as executed. A lane whose every test skips (for example when the packaged-server test is skipped because a server was supplied) now reports `not_run` instead of passing (#2528).

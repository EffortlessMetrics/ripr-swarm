<!-- section: Internal / CI -->
- The sharded Windows advisory lane now also warms `cargo xtask check-file-policy` before its tests, so the facade policy test no longer pays that command's cold combined test build (about 12 minutes on windows-latest) inside its 300 s child cap (#12035).

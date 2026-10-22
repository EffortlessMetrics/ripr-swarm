<!-- section: Internal / CI -->
- LSP initialize-failure unit tests now run on a paused clock, so three tests no longer wait out the 2 s client-disclosure budget, and the disclosure-omitted comment says only tests read that flag (#9324).

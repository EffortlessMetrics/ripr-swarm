<!-- section: Internal / CI -->
- The Windows force-terminated-owner control now also ends the unbound owned primary it leaves behind, so the test no longer holds the harness's output pipe for the primary's remaining 40 s and nextest stops reporting it as LEAK (#9564).

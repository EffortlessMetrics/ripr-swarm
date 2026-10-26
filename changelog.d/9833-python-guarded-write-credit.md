<!-- section: Fixed -->
- Python: a changed `self.attr = ...` line under a plain `if` guard in an ordinary method is credited again when every strong related call provably enters the guard; a skipped guard, an `elif`/`else`/loop arm or an earlier exit still reads no effect (#9833).
- Guard credit uses Python tokens to retain literal/comment positives while refusing escaped exits, input or receiver rebinding, ambiguous indentation and explicit later channel writes (#9868).
- Keep read-only field uses credited and refuse loop/comprehension field masks in guarded-write retention. (#9833, #9868)

<!-- section: Fixed -->
- Python: a changed `self.attr = ...` line under a plain `if` guard in an ordinary method is credited again when every strong related call provably enters the guard; a skipped guard, an `elif`/`else`/loop arm or an earlier exit still reads no effect (#9833).

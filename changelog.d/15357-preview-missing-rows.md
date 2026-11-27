<!-- section: Fixed -->
- Python and TypeScript weakly exposed rows now name a concrete missing check
  where the source shows one, so `check --format agent` prints the gap row
  instead of only a header count. A changed entry of a multi-line
  Python `return {` literal names an exact read of that key
  (`result["Accept-Encoding"] == "gzip, deflate, br"`); a changed TypeScript
  `const x = options.p ?? <literal>` default names the call that omits `p`
  when every call read under a strong assertion passes it. Both stay
  `weakly_exposed`; the `??` reader also accepts plain `const`/`let`
  declarations and an `options = {}` parameter default, which still reads
  nullish (#15357).

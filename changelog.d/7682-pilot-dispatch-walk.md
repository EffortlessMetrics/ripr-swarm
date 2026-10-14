<!-- section: Changed -->
- Cold `ripr pilot` runs on large workspaces are faster. The trait-dispatch
  index that every evidence worker waits on now walks the call graph over
  interned names and tracks reached functions by address, instead of hashing
  every call name on each visit and comparing id strings. In one cold run
  each against the same main build, pilot was 2.4 s faster on bevy and 4.3 s
  faster on nushell, with identical output (#7682).

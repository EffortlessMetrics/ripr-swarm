<!-- section: Changed -->
- Cold `ripr pilot` runs on large workspaces are faster. The trait-dispatch
  index that every evidence worker waits on now walks the call graph over
  interned names instead of hashing every call name on each visit. Cold pilot
  drops from 29.7 s to 27.3 s on bevy and 39.4 s to 35.1 s on nushell, with
  identical output (#7682).

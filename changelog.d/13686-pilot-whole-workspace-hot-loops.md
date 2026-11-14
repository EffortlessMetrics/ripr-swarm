<!-- section: Changed -->
- `ripr pilot` is faster on large workspaces: an enum's declaring file is
  decided once per index rather than on every seam and test admission, and
  the transitive-caller walk uses the prebuilt reverse call map. Measured on
  a release build with 15 files edited, pilot on bevy went from a partial
  result at its 275 s budget to a complete one in about 56 s, and on a polars
  shallow clone from partial at 275 s to complete in about 40 s. On polars
  the cold artifacts are byte-identical to the previous build given an
  unlimited budget (#13686).

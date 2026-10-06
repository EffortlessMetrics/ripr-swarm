<!-- section: Fixed -->
GitHub annotations for files whose names contain control or bidi characters no longer carry a `file=` property that names a nonexistent file; the annotation drops `file=`/`line=` and names the escaped location in its message (#6309).

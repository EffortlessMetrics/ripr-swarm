<!-- section: Fixed -->
- A Python `isinstance` guard is also read as unknown when the file rebinds a builtin through the builtins module with an annotated assignment (`b.isinstance: object = f`) or a tuple, list or multi-target assignment (`b.isinstance, other = f, g`); a bare annotation, a read in a tuple and a comparison still leave the guard readable (#12222).

<!-- section: Fixed -->
- A Python `isinstance` guard is read as unknown, and keeps its credit, when the file rewrites a builtin through the `builtins` module (`builtins.isinstance = f`, `setattr(builtins, ..)`) (#12183).

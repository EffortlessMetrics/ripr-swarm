<!-- section: Fixed -->
- A Python `isinstance` guard is also read as unknown when the file reaches the builtins module through an `import builtins as <alias>`, `__builtins__`, `vars(builtins)`, a `setattr` call split across lines, or a spaced `builtins . name = f` (#12206).

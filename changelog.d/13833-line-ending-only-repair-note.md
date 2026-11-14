<!-- section: Changed -->
- Repair after phase: a file whose only change is its line endings, on a path Git normalizes (`core.autocrlf` or a `text` attribute), is no longer silently missing from the result. The after phase names it as "not counted as edits" and records it in the attempt manifest (#13833).

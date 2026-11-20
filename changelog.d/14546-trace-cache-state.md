<!-- section: Internal / CI -->
- Make the resource trace output comparison use private cache locations and verify matching cold and warm states. Cache-miss and unrelated diagnostics remain compared; parallel tests cannot supply one side's cache state (#14546).
- Recognize the existing opt-in file-fact cache receipt by its exact reserved prefix in the trace comparison; keep near-miss prefixes and ordinary diagnostics visible (#14546).

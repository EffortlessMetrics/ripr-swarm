<!-- section: Changed -->
- `ripr agent repair` (and `ripr repair`) start step: without `--json`, stderr no longer repeats the next command, the attempt manifest path and the edit directive that the stdout summary already prints, so the start step is 15 lines instead of 20 and the next command appears once (#13803).

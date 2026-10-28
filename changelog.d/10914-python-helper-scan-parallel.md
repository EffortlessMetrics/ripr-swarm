<!-- section: Changed -->
- Python: the weak helper-caller relation now scans tests in parallel on large workspaces, with the same first-caller attribution. A warm check on pandas ecdb4258 (2 changed files, ~23k tests) dropped from 15.7 s to 12.0–13.3 s, and on Django 5c13b5a from 8.5–9.1 s to 7.3–7.7 s, both with byte-identical output (#10914).

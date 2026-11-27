<!-- section: Changed -->
- `ripr pr-evidence --check` and `ripr impacted-evidence --check` now say which command writes a missing or unreadable packet, and a broken `repo-exposure.json` says to regenerate it. When git is missing inside a work tree, the cache-trust refusal says to install git or add it to PATH (#16019).

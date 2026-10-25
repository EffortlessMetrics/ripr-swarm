<!-- section: Changed -->
- A warm `ripr check` no longer parses a source file that parsed cleanly and never spells `doc` followed by `=` to look for an external-docs attribute (#9794). On cargo's pinned commit this removes about 240 of 287 parses, with byte-identical findings and about 15 MB less peak memory. A file that did not parse cleanly is still scanned and still answers conservatively.

<!-- section: Fixed -->
- Python findings for identical added lines at one owner no longer share one probe ID: they take `.2`, `.3` suffixes like Rust and TypeScript, ranked by a full payload digest so lines that merely share an eight-hex fingerprint keep their IDs whichever comes first. Lone findings keep their IDs (#10450, #8838).

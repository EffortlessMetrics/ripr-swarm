<!-- section: Fixed -->
- The language server no longer re-extracts every unchanged TypeScript test file on each save: extraction is served from a content-keyed cache and committed snapshots are still read on every run. Re-saving a small file next to a 2 MB test file went from about 7.7–8.6 s to about 4.3–4.8 s in a debug build (#10017).

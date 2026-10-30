<!-- section: Changed -->
- The macro-binding site scan walks a short per-file list of the nodes it can react to, built in the pass that already indexes the file, instead of walking every syntax node once per queried name set (#9794). On cargo's pinned commit a warm `ripr check` is about 13% faster in paired runs (CPU −1.1 s) with byte-identical findings and about the same peak memory.

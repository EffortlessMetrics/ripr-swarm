<!-- section: Changed -->
- A check no longer re-parses a Rust file for its module-tree edges when the module graph, the call-identity owner lookup and the facts build ask about the same exact source text. The scan is memoized by full text under an 8 MB key budget, so memory stays flat and findings are byte-identical (#9794).

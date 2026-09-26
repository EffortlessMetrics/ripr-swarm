// The cache producers and both cache-management commands share this registry.
// Adding a producer layer here also adds it to the clear/report ownership set.
macro_rules! cache_layers {
    ($($variant:ident => $name:literal),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub(crate) enum CacheLayer {
            $($variant),+
        }

        impl CacheLayer {
            pub(crate) const ALL: &'static [Self] = &[$(Self::$variant),+];

            pub(crate) const fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => $name),+
                }
            }
        }
    };
}

cache_layers! {
    SeamFacts => "repo-seam-facts",
    SeamFactsSharded => "repo-seam-facts-sharded",
    CompactClassifiedSeams => "repo-compact-classified-seams",
    CompactClassifiedSeamsSharded => "repo-compact-classified-seams-sharded",
    CorpusFingerprint => "repo-corpus-fingerprint",
    FileFacts => "repo-file-facts",
    SeamCounts => "repo-seam-counts",
}

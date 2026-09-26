//! Cache directory names shared by the producers and the two cache operators.
//!
//! Keep the enumeration and its inventory in one declaration. Adding a layer
//! for a producer also makes it visible to `ripr cache clear` and xtask's
//! relocated-root check; neither operator grants ownership of the parent.

macro_rules! cache_layers {
    ($($variant:ident => $name:literal),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub enum CacheLayer {
            $($variant),+
        }

        pub const OWNED_CACHE_LAYERS: &[CacheLayer] = &[$(CacheLayer::$variant),+];

        impl CacheLayer {
            pub const fn name(self) -> &'static str {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn producer_layers_are_named_once_and_registered_for_operators() {
        let names = OWNED_CACHE_LAYERS
            .iter()
            .map(|layer| layer.name())
            .collect::<BTreeSet<_>>();
        assert_eq!(names.len(), OWNED_CACHE_LAYERS.len());

        // A new producer path must use a typed layer. If it instead creates
        // a raw directory, clear and xtask may silently omit it (#3928).
        let producer = include_str!("analysis/seam_cache.rs");
        assert!(
            !producer.contains(".join(\"repo-"),
            "a cache producer bypassed the shared cache-layer inventory"
        );
    }
}

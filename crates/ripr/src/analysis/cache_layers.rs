//! Direct children created by the analysis cache under its resolved root.
//!
//! Shared with xtask's cache reporter. The single declaration generates both
//! producer names and the complete set recognized by cache maintenance.

macro_rules! cache_layers {
    ($( $variant:ident => $name:literal ),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub(crate) enum CacheLayer {
            $( $variant ),+
        }

        impl CacheLayer {
            pub(crate) const ALL: &'static [Self] = &[ $( Self::$variant ),+ ];

            pub(crate) const fn name(self) -> &'static str {
                match self {
                    $( Self::$variant => $name ),+
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
    use super::CacheLayer;
    use std::collections::BTreeSet;

    #[test]
    fn cache_layer_names_are_unique_direct_children() {
        let names: Vec<_> = CacheLayer::ALL.iter().map(|layer| layer.name()).collect();
        let unique: BTreeSet<_> = names.iter().copied().collect();
        assert_eq!(names.len(), unique.len());
        assert!(names.iter().all(|name| {
            !name.is_empty()
                && !name.starts_with('.')
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        }));
    }

    #[test]
    fn producer_cannot_add_an_unregistered_literal_layer() {
        // A new `.join("repo-new-layer")` in the producer must fail this
        // control rather than silently escaping cache clear and xtask GC.
        let producer = include_str!("seam_cache.rs");
        assert!(!producer.contains(".join(\"repo-"));
        assert!(!producer.contains("at_named(workspace_root, \""));
    }
}

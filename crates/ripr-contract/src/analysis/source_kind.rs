//! Exact TypeScript/JavaScript source-extension identity.

/// Canonical TypeScript source extensions (RIPR #4116). One owner lists the
/// routed TS/JS suffixes so downstream consumers (repair route, targeted
/// rerun, packet projection, first-use output, LSP, doctor) cannot drift
/// from the router's surface.
pub const TYPESCRIPT_SOURCE_EXTENSIONS: &[&str] = &["ts", "tsx", "mts", "cts"];

/// Canonical JavaScript source extensions (RIPR #4116); see
/// [`TYPESCRIPT_SOURCE_EXTENSIONS`].
pub const JAVASCRIPT_SOURCE_EXTENSIONS: &[&str] = &["js", "jsx", "mjs", "cjs"];

/// Which side of the TypeScript/JavaScript source family an extension
/// belongs to. `.mts`/`.cts` are TypeScript; `.mjs`/`.cjs` are JavaScript.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TsJsSourceKind {
    TypeScript,
    JavaScript,
}

/// Classify an exact file extension into the canonical TypeScript/JavaScript
/// source family. Matching is exact against the shared lists — never
/// substring or final-character based — so near-misses such as `.mt`,
/// `.cj`, `.mjsx`, or `.ctsx` and every non-TS/JS extension return `None`.
/// Callers that historically folded case normalize before calling.
pub fn ts_js_source_kind(extension: &str) -> Option<TsJsSourceKind> {
    if TYPESCRIPT_SOURCE_EXTENSIONS.contains(&extension) {
        Some(TsJsSourceKind::TypeScript)
    } else if JAVASCRIPT_SOURCE_EXTENSIONS.contains(&extension) {
        Some(TsJsSourceKind::JavaScript)
    } else {
        None
    }
}

/// Whether an exact file extension is one of the eight routed TS/JS source
/// extensions; see [`ts_js_source_kind`].
pub fn is_ts_js_source_extension(extension: &str) -> bool {
    ts_js_source_kind(extension).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ts_js_extension_authority_classifies_exactly_the_routed_surface() {
        // #4116 removal control: deleting any extension from the shared
        // authority must flip at least one of these parity rows.
        let cases = [
            ("ts", Some(TsJsSourceKind::TypeScript)),
            ("tsx", Some(TsJsSourceKind::TypeScript)),
            ("mts", Some(TsJsSourceKind::TypeScript)),
            ("cts", Some(TsJsSourceKind::TypeScript)),
            ("js", Some(TsJsSourceKind::JavaScript)),
            ("jsx", Some(TsJsSourceKind::JavaScript)),
            ("mjs", Some(TsJsSourceKind::JavaScript)),
            ("cjs", Some(TsJsSourceKind::JavaScript)),
        ];

        for (extension, expected) in cases {
            assert_eq!(ts_js_source_kind(extension), expected, ".{extension}");
            assert!(is_ts_js_source_extension(extension), ".{extension}");
        }

        // Near-misses and unrelated extensions stay unknown; matching is
        // exact, never substring or final-character based.
        for extension in ["mt", "cj", "mjsx", "ctsx", "ts2", "tsx?", "py", "rs", ""] {
            assert_eq!(ts_js_source_kind(extension), None, ".{extension}");
            assert!(!is_ts_js_source_extension(extension), ".{extension}");
        }
    }
}

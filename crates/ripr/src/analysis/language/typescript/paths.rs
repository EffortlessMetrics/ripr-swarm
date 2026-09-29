//! Path utilities for the TypeScript preview adapter.

use super::*;

pub(crate) fn normalized_path(path: &Path) -> String {
    let mut normalized = path.to_string_lossy().replace('\\', "/");
    while let Some(stripped) = normalized.strip_prefix("./") {
        normalized = stripped.to_string();
    }
    normalized
}

pub(crate) fn output_language_for(path: &Path) -> DomainLanguageId {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("js" | "jsx" | "mjs" | "cjs") => DomainLanguageId::JavaScript,
        _ => DomainLanguageId::TypeScript,
    }
}

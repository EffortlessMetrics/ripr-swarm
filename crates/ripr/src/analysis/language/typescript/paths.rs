//! Path utilities for the TypeScript preview adapter.

use super::*;

/// 1-indexed line for a 0-indexed byte offset.
pub(crate) fn line_for_offset(source: &str, offset: usize) -> usize {
    let mut line: usize = 1;
    for (idx, ch) in source.char_indices() {
        if idx >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
        }
    }
    line
}

/// Whether an import specifier is relative: `./x`, `../x`, or the bare
/// current / parent directory `.` / `..` (`require('..')`, #4546). The one
/// predicate every resolver and disclosure uses, so `.` / `..` cannot be
/// relative to one site and a bare package or alias to another.
pub(crate) fn is_relative_specifier(specifier: &str) -> bool {
    specifier == "."
        || specifier == ".."
        || specifier.starts_with("./")
        || specifier.starts_with("../")
}

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

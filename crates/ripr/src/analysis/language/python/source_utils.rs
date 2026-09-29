use crate::analysis::language::IndexedSource;
use rustpython_parser::text_size::TextRange;
use std::path::Path;

pub(super) fn line_for_range_start(source: &IndexedSource<'_>, range: TextRange) -> usize {
    source.line(usize::from(range.start()))
}

pub(super) fn line_for_range_end(source: &IndexedSource<'_>, range: TextRange) -> usize {
    source.line(usize::from(range.end()))
}

pub(super) fn text_for_range(source: &str, range: TextRange) -> String {
    let start = usize::from(range.start()).min(source.len());
    let end = usize::from(range.end()).min(source.len());
    source.get(start..end).unwrap_or_default().to_string()
}

pub(super) fn normalized_path(path: &Path) -> String {
    let mut normalized = path.to_string_lossy().replace('\\', "/");
    while let Some(stripped) = normalized.strip_prefix("./") {
        normalized = stripped.to_string();
    }
    normalized
}

pub(super) fn is_test_file(path: &Path) -> bool {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if file_name.starts_with("test_") || file_name.ends_with("_test.py") {
        return true;
    }
    path.components().any(|component| {
        let text = component.as_os_str().to_string_lossy();
        text == "tests" || text == "test"
    })
}

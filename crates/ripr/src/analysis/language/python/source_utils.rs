use rustpython_parser::text_size::TextRange;
use std::path::Path;

/// 1-indexed line for a 0-indexed byte offset.
pub(super) fn line_for_offset(source: &str, offset: usize) -> usize {
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

/// A source plus its newline byte offsets, so a line lookup is a binary
/// search instead of a scan from the start of the file. Source-fact
/// extraction looks up two lines per fact, and the scan made it quadratic
/// in file size (~79% of a `ripr check` on a Django commit). Lines match
/// `line_for_offset` exactly: a `\n` byte is never part of a multi-byte
/// UTF-8 character, so counting newline bytes before `offset` counts the
/// same newlines as counting newline characters there.
pub(super) struct IndexedSource<'a> {
    text: &'a str,
    newline_offsets: Vec<usize>,
}

impl<'a> IndexedSource<'a> {
    pub(super) fn new(text: &'a str) -> Self {
        let newline_offsets = text
            .bytes()
            .enumerate()
            .filter_map(|(offset, byte)| (byte == b'\n').then_some(offset))
            .collect();
        Self {
            text,
            newline_offsets,
        }
    }

    /// `line_for_offset(self, offset)`.
    pub(super) fn line_for_offset(&self, offset: usize) -> usize {
        1 + self
            .newline_offsets
            .partition_point(|newline| *newline < offset)
    }

    pub(super) fn line_for_range_start(&self, range: TextRange) -> usize {
        self.line_for_offset(usize::from(range.start()))
    }

    pub(super) fn line_for_range_end(&self, range: TextRange) -> usize {
        self.line_for_offset(usize::from(range.end()))
    }
}

impl std::ops::Deref for IndexedSource<'_> {
    type Target = str;

    fn deref(&self) -> &str {
        self.text
    }
}

pub(super) fn line_for_range_start(source: &str, range: TextRange) -> usize {
    line_for_offset(source, usize::from(range.start()))
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

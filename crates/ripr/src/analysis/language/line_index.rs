//! Offset-to-line lookup for preview-adapter fact extraction.
//!
//! Owner and test extraction asks for the line of two spans per owner. A
//! scan from the start of the file for each lookup made a file with many
//! owners quadratic: an untouched 1.3 MB JavaScript file with 30 000
//! functions added about 30 s to every TypeScript check, and a 1.7 MB Python
//! module almost 5 minutes. The index is built once per parsed source.

use std::ops::Deref;

/// Byte offsets where each line of a source starts.
#[derive(Debug)]
pub(crate) struct LineIndex {
    starts: Vec<usize>,
}

impl LineIndex {
    pub(crate) fn new(source: &str) -> Self {
        let starts = std::iter::once(0)
            .chain(source.match_indices('\n').map(|(offset, _)| offset + 1))
            .collect();
        Self { starts }
    }

    /// 1-indexed line holding byte `offset`: one plus the newlines before
    /// it. An offset past the end reports the last line.
    pub(crate) fn line(&self, offset: usize) -> usize {
        self.starts.partition_point(|&start| start <= offset)
    }
}

/// A parsed source with its line index. Derefs to the text, so extraction
/// code keeps slicing and passing it as `&str`.
#[derive(Debug)]
pub(crate) struct IndexedSource<'a> {
    text: &'a str,
    lines: LineIndex,
}

impl<'a> IndexedSource<'a> {
    pub(crate) fn new(text: &'a str) -> Self {
        Self {
            text,
            lines: LineIndex::new(text),
        }
    }

    /// 1-indexed line holding byte `offset`.
    pub(crate) fn line(&self, offset: usize) -> usize {
        self.lines.line(offset)
    }
}

impl Deref for IndexedSource<'_> {
    type Target = str;

    fn deref(&self) -> &str {
        self.text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The scan every caller used before the index; the index must agree
    /// with it at every offset, including past the end.
    fn scanned_line(source: &str, offset: usize) -> usize {
        1 + source
            .char_indices()
            .take_while(|(index, _)| *index < offset)
            .filter(|(_, ch)| *ch == '\n')
            .count()
    }

    #[test]
    fn index_matches_a_scan_at_every_offset() {
        for source in ["", "a", "\n", "ab\ncd\n", "\n\nx\r\ny", "é\nß\n\n€z"] {
            let index = LineIndex::new(source);
            for offset in 0..=source.len() + 2 {
                assert_eq!(
                    index.line(offset),
                    scanned_line(source, offset),
                    "offset {offset} of {source:?}"
                );
            }
        }
    }

    #[test]
    fn indexed_source_reads_as_its_text() {
        let source = IndexedSource::new("fn a\nfn b\n");
        assert_eq!(&source[5..9], "fn b");
        assert_eq!(source.line(5), 2);
    }
}

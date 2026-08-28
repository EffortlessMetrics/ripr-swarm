from pathlib import Path

path = Path("crates/ripr/src/lsp/tests.rs")
text = path.read_text(encoding="utf-8")

replacements = [
    (
        '''    store.change(DidChangeTextDocumentParams {
        text_document: VersionedTextDocumentIdentifier::new(uri.clone(), 2),
        content_changes: vec![TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: "fn new() {}".to_string(),
        }],
    });
''',
        '''    store.change(
        DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier::new(uri.clone(), 2),
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: "fn new() {}".to_string(),
            }],
        },
        &tower_lsp_server::ls_types::PositionEncodingKind::UTF16,
    );
''',
    ),
    (
        '''    store.change(DidChangeTextDocumentParams {
        text_document: VersionedTextDocumentIdentifier::new(uri.clone(), 7),
        content_changes: vec![TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: "fn discovered() {}".to_string(),
        }],
    });
''',
        '''    store.change(
        DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier::new(uri.clone(), 7),
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: "fn discovered() {}".to_string(),
            }],
        },
        &tower_lsp_server::ls_types::PositionEncodingKind::UTF16,
    );
''',
    ),
]

for old, new in replacements:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"expected one compatibility-test anchor, found {count}")
    text = text.replace(old, new, 1)

path.write_text(text, encoding="utf-8")

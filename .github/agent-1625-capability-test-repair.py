from pathlib import Path

path = Path("crates/ripr/src/lsp/tests.rs")
text = path.read_text(encoding="utf-8")
old = '''    assert_eq!(
        result.capabilities.text_document_sync,
        Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL))
    );
'''
new = '''    let Some(TextDocumentSyncCapability::Options(sync)) =
        result.capabilities.text_document_sync.as_ref()
    else {
        return Err("expected explicit text-document sync options".to_string());
    };
    assert_eq!(sync.open_close, Some(true));
    assert_eq!(sync.change, Some(TextDocumentSyncKind::INCREMENTAL));
'''
if text.count(old) != 1:
    raise SystemExit(f"expected one stale capability assertion, found {text.count(old)}")
path.write_text(text.replace(old, new, 1), encoding="utf-8")

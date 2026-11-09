//! Handler-level proof for the store-wide open-document and retained-byte
//! caps (#11666). The `DocumentStore` unit tests prove the policy; these drive
//! the real `didOpen` / `didChange` / `didClose` handlers on a `Backend` so a
//! refused open is shown to quarantine the overflow URI (empty text,
//! `buffer_authority_unknown`) and to keep later notifications working.
//!
//! Store occupancy is staged directly through `with_documents_for_test` to
//! avoid thousands of handler round trips; only the notifications under test
//! go through the handlers.

use super::*;
use crate::lsp::state::{DOCUMENT_STORE_LIMITS, DocumentStore, DocumentStoreLimits};

fn filler_uri(index: usize) -> Result<tower_lsp_server::ls_types::Uri, String> {
    format!("file:///ripr-overflow-filler/{index}.rs")
        .parse::<tower_lsp_server::ls_types::Uri>()
        .map_err(|err| format!("filler URI failed: {err}"))
}

fn close_params(uri: &tower_lsp_server::ls_types::Uri) -> DidCloseTextDocumentParams {
    DidCloseTextDocumentParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
    }
}

fn incremental_change_params(
    uri: &tower_lsp_server::ls_types::Uri,
    version: i32,
) -> DidChangeTextDocumentParams {
    DidChangeTextDocumentParams {
        text_document: VersionedTextDocumentIdentifier::new(uri.clone(), version),
        content_changes: vec![TextDocumentContentChangeEvent {
            range: Some(Range::new(Position::new(0, 0), Position::new(0, 2))),
            range_length: None,
            text: "FN".to_string(),
        }],
    }
}

fn require_overflow_placeholder(
    backend: &Backend,
    uri: &tower_lsp_server::ls_types::Uri,
    stage: &str,
) -> Result<(), String> {
    let state = backend
        .document_state_for_test(uri)
        .ok_or_else(|| format!("{stage}: overflow URI must stay in the store"))?;
    if !state.text.is_empty() {
        return Err(format!(
            "{stage}: overflow text must stay empty, got {:?}",
            state.text
        ));
    }
    if !state.buffer_authority_unknown {
        return Err(format!(
            "{stage}: overflow must pin buffer_authority_unknown"
        ));
    }
    if state
        .quarantine
        .as_ref()
        .map(|quarantine| quarantine.reason)
        != Some(DocumentStalenessReason::DocumentStoreLimitReached)
    {
        return Err(format!(
            "{stage}: overflow must sit in the fail-closed quarantine"
        ));
    }
    Ok(())
}

async fn require_withdrawn_not_served(
    backend: &Backend,
    uri: &tower_lsp_server::ls_types::Uri,
    stage: &str,
) -> Result<(), String> {
    let report = pull_document_json(backend, uri, None).await?;
    if !report_is_quarantine_withdrawal(&report) {
        return Err(format!(
            "{stage}: overflow URI must serve only the withdrawal marker: {report}"
        ));
    }
    if report
        .pointer("/items/0/data/reason")
        .and_then(serde_json::Value::as_str)
        != Some("document_store_limit_reached")
    {
        return Err(format!(
            "{stage}: marker must name the document-store limit: {report}"
        ));
    }
    let message = report
        .pointer("/items/0/message")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if !message.contains("open-document limit was reached") || message.contains("incremental") {
        return Err(format!(
            "{stage}: marker text must name the limit, not an edit failure: {message}"
        ));
    }
    let status = workspace_status_json(backend).await?;
    let entry = open_document_entry(&status, uri.as_str())?;
    if entry
        .get("staleness_reason")
        .and_then(serde_json::Value::as_str)
        != Some("document_store_limit_reached")
    {
        return Err(format!(
            "{stage}: workspace status must name the document-store limit: {entry}"
        ));
    }
    if entry.get("state").and_then(serde_json::Value::as_str) == Some("clean")
        || entry
            .get("line_local_diagnostics")
            .and_then(serde_json::Value::as_str)
            == Some("served")
    {
        return Err(format!(
            "{stage}: overflow URI must not read as clean/served: {entry}"
        ));
    }
    Ok(())
}

#[tokio::test]
async fn did_open_beyond_the_document_count_cap_is_refused_and_quarantined() -> Result<(), String> {
    let fixture = quarantine_fixture("docstore-count-overflow")?;
    let (service, socket) = LspService::new(|client| Backend::new(client, fixture.root.clone()));
    drop(socket);
    let backend = service.inner();
    if DOCUMENT_STORE_LIMITS.max_open_documents != 4_096 {
        return Err("SETUP: production document-count cap is no longer 4,096".into());
    }
    let cap = DOCUMENT_STORE_LIMITS.max_open_documents;
    backend
        .with_documents_for_test(|store| {
            for index in 0..cap {
                if let Ok(uri) = filler_uri(index) {
                    store.open(quarantine_open_params(&uri, "fn f() {}\n"));
                }
            }
        })
        .ok_or_else(|| "SETUP: document store lock failed".to_string())?;
    let last_filler = filler_uri(cap - 1)?;
    if backend.document_state_for_test(&last_filler).is_none() {
        return Err("SETUP: the store was not filled to the production cap".into());
    }

    // The real didOpen path refuses the 4,097th accepted document.
    backend
        .did_open(quarantine_open_params(&fixture.uri_a, QUARANTINE_TEXT_A))
        .await;
    require_overflow_placeholder(backend, &fixture.uri_a, "after refused didOpen")?;
    // A committed snapshot carries real diagnostics for the URI; the refused
    // buffer must still withdraw them rather than serve them.
    commit_quarantine_snapshot(backend, &fixture, QUARANTINE_TEXT_A)?;
    require_withdrawn_not_served(backend, &fixture.uri_a, "after refused didOpen").await?;

    // A full-replacement and an incremental didChange cannot restore authority
    // past the cap, and neither disturbs the accepted documents.
    backend
        .did_change(quarantine_change_params(
            &fixture.uri_a,
            2,
            QUARANTINE_TEXT_A_DIRTY,
        ))
        .await;
    require_overflow_placeholder(backend, &fixture.uri_a, "after full-replacement didChange")?;
    backend
        .did_change(incremental_change_params(&fixture.uri_a, 3))
        .await;
    require_overflow_placeholder(backend, &fixture.uri_a, "after incremental didChange")?;
    let kept = backend
        .document_state_for_test(&last_filler)
        .ok_or_else(|| "an accepted document was evicted by the refusal".to_string())?;
    if kept.text != "fn f() {}\n" || kept.buffer_authority_unknown {
        return Err("an accepted document was altered by the refusal".into());
    }

    // didClose of an accepted document frees a slot, so reopening the overflow
    // URI is accepted with its real text and authority.
    backend.did_close(close_params(&last_filler)).await;
    if backend.document_state_for_test(&last_filler).is_some() {
        return Err("didClose did not remove the accepted document".into());
    }
    backend
        .did_open(quarantine_open_params(&fixture.uri_a, QUARANTINE_TEXT_A))
        .await;
    let reopened = backend
        .document_state_for_test(&fixture.uri_a)
        .ok_or_else(|| "reopened URI missing".to_string())?;
    if reopened.text != QUARANTINE_TEXT_A || reopened.buffer_authority_unknown {
        return Err("a freed slot did not admit the reopened document".into());
    }

    // didClose of an overflow placeholder removes it.
    let second = fixture.uri_b.clone();
    backend
        .did_open(quarantine_open_params(&second, QUARANTINE_TEXT_B))
        .await;
    require_overflow_placeholder(backend, &second, "second overflow")?;
    backend.did_close(close_params(&second)).await;
    if backend.document_state_for_test(&second).is_some() {
        return Err("didClose did not remove the overflow placeholder".into());
    }
    Ok(())
}

#[tokio::test]
async fn did_open_beyond_the_retained_byte_cap_is_refused_and_quarantined() -> Result<(), String> {
    let fixture = quarantine_fixture("docstore-byte-overflow")?;
    let (service, socket) = LspService::new(|client| Backend::new(client, fixture.root.clone()));
    drop(socket);
    let backend = service.inner();
    if DOCUMENT_STORE_LIMITS.max_retained_document_bytes != 512 * 1024 * 1024 {
        return Err("SETUP: production retained-byte cap is no longer 512 MiB".into());
    }
    // The production cap is 512 MiB; a tiny store stages the same refusal
    // without allocating it. The unit tests in state.rs pin the policy itself.
    let filler = filler_uri(0)?;
    backend
        .with_documents_for_test(|store| {
            *store = DocumentStore::with_limits(DocumentStoreLimits {
                max_open_documents: 8,
                max_retained_document_bytes: QUARANTINE_TEXT_A.len() + 16,
            });
            store.open(quarantine_open_params(&filler, "fn filler() {}\n\n\n"));
        })
        .ok_or_else(|| "SETUP: document store lock failed".to_string())?;
    if backend
        .document_state_for_test(&filler)
        .is_none_or(|state| {
            state.text.len() + QUARANTINE_TEXT_A.len() <= QUARANTINE_TEXT_A.len() + 16
        })
    {
        return Err("SETUP: filler does not leave less headroom than the fixture needs".into());
    }

    backend
        .did_open(quarantine_open_params(&fixture.uri_a, QUARANTINE_TEXT_A))
        .await;
    require_overflow_placeholder(backend, &fixture.uri_a, "after refused didOpen")?;
    // A committed snapshot carries real diagnostics for the URI; the refused
    // buffer must still withdraw them rather than serve them.
    commit_quarantine_snapshot(backend, &fixture, QUARANTINE_TEXT_A)?;
    require_withdrawn_not_served(backend, &fixture.uri_a, "after refused didOpen").await?;

    backend
        .did_change(quarantine_change_params(
            &fixture.uri_a,
            2,
            QUARANTINE_TEXT_A_DIRTY,
        ))
        .await;
    require_overflow_placeholder(backend, &fixture.uri_a, "after didChange over the byte cap")?;

    backend.did_close(close_params(&fixture.uri_a)).await;
    if backend.document_state_for_test(&fixture.uri_a).is_some() {
        return Err("didClose did not remove the byte-overflow placeholder".into());
    }

    // Freeing the retained bytes admits the same document.
    backend.did_close(close_params(&filler)).await;
    backend
        .did_open(quarantine_open_params(&fixture.uri_a, QUARANTINE_TEXT_A))
        .await;
    let reopened = backend
        .document_state_for_test(&fixture.uri_a)
        .ok_or_else(|| "reopened URI missing".to_string())?;
    if reopened.text != QUARANTINE_TEXT_A || reopened.buffer_authority_unknown {
        return Err("freed retained bytes did not admit the reopened document".into());
    }
    Ok(())
}

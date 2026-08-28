from pathlib import Path
import re


def replace_once(path: str, old: str, new: str) -> None:
    file_path = Path(path)
    text = file_path.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one replacement anchor, found {count}")
    file_path.write_text(text.replace(old, new, 1), encoding="utf-8")


def regex_insert_once(path: str, pattern: str, insertion: str) -> None:
    file_path = Path(path)
    text = file_path.read_text(encoding="utf-8")
    matches = list(re.finditer(pattern, text, flags=re.DOTALL))
    if len(matches) != 1:
        raise SystemExit(f"{path}: expected one regex anchor, found {len(matches)}")
    match = matches[0]
    updated = text[: match.end()] + insertion + text[match.end() :]
    file_path.write_text(updated, encoding="utf-8")


# Advertise the document lifecycle ripr actually implements.
replace_once(
    "crates/ripr/src/lsp/capabilities.rs",
    """    CodeActionKind, CodeActionOptions, CodeActionProviderCapability, CodeLensOptions,
    DiagnosticOptions, DiagnosticServerCapabilities, ExecuteCommandOptions,
    HoverProviderCapability, InitializeParams, InitializeResult, OneOf, PositionEncodingKind,
    ServerCapabilities, ServerInfo, TextDocumentSyncCapability, TextDocumentSyncKind,
    WorkspaceFoldersServerCapabilities, WorkspaceServerCapabilities,
""",
    """    CodeActionKind, CodeActionOptions, CodeActionProviderCapability, CodeLensOptions,
    DiagnosticOptions, DiagnosticServerCapabilities, ExecuteCommandOptions,
    HoverProviderCapability, InitializeParams, InitializeResult, OneOf, PositionEncodingKind,
    SaveOptions, ServerCapabilities, ServerInfo, TextDocumentSyncCapability,
    TextDocumentSyncKind, TextDocumentSyncOptions, TextDocumentSyncSaveOptions,
    WorkspaceFoldersServerCapabilities, WorkspaceServerCapabilities,
""",
)
replace_once(
    "crates/ripr/src/lsp/capabilities.rs",
    """            text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
""",
    """            // RIPR-SPEC-0129 / #1625: the server owns one current
            // in-memory buffer identity per open document, replays ordered
            // incremental changes, and never asks clients to resend full
            // source text on every edit or include source text on save.
            text_document_sync: Some(TextDocumentSyncCapability::Options(
                TextDocumentSyncOptions {
                    open_close: Some(true),
                    change: Some(TextDocumentSyncKind::INCREMENTAL),
                    will_save: Some(false),
                    will_save_wait_until: Some(false),
                    save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                        include_text: Some(false),
                    })),
                },
            )),
""",
)
replace_once(
    "crates/ripr/src/lsp/capabilities.rs",
    """    #[test]
    fn work_done_progress_requires_window_capability() -> Result<(), String> {
""",
    """    #[test]
    fn initialize_result_advertises_explicit_incremental_document_sync() -> Result<(), String> {
        let Some(TextDocumentSyncCapability::Options(options)) =
            initialize_result().capabilities.text_document_sync
        else {
            return Err("expected explicit textDocumentSync options".to_string());
        };
        if options.open_close != Some(true)
            || options.change != Some(TextDocumentSyncKind::INCREMENTAL)
            || options.will_save != Some(false)
            || options.will_save_wait_until != Some(false)
        {
            return Err("incremental document lifecycle options drifted".to_string());
        }
        match options.save {
            Some(TextDocumentSyncSaveOptions::SaveOptions(options))
                if options.include_text == Some(false) => {}
            _ => {
                return Err(
                    "didSave must be supported without requesting full source text".to_string(),
                );
            }
        }
        Ok(())
    }

    #[test]
    fn work_done_progress_requires_window_capability() -> Result<(), String> {
""",
)

# Replay incremental changes inside the existing server-owned document store.
replace_once(
    "crates/ripr/src/lsp/state.rs",
    """use tower_lsp_server::ls_types::{
    Diagnostic, DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    Uri, WorkspaceFolder,
};
""",
    """use tower_lsp_server::ls_types::{
    Diagnostic, DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    Position, PositionEncodingKind, Range, TextDocumentContentChangeEvent, Uri, WorkspaceFolder,
};
""",
)
replace_once(
    "crates/ripr/src/lsp/state.rs",
    """pub(super) enum DocumentStalenessReason {
    /// The open buffer diverges from the saved content the committed
""",
    """pub(super) enum DocumentStalenessReason {
    /// The server could not reconstruct the latest buffer from the received
    /// incremental change sequence. Line-local evidence must fail closed
    /// until a complete replacement or reopen establishes current text.
    BufferContentUnavailable,
    /// The open buffer diverges from the saved content the committed
""",
)
regex_insert_once(
    "crates/ripr/src/lsp/state.rs",
    r"fn as_str\(self\) -> &'static str \{\n\s*match self \{\n",
    '            Self::BufferContentUnavailable => "buffer_content_unavailable",\n',
)
regex_insert_once(
    "crates/ripr/src/lsp/state.rs",
    r"fn description\(self\) -> &'static str \{\n\s*match self \{\n",
    """            Self::BufferContentUnavailable => {
                "the server could not reconstruct the current buffer from document changes"
            }
""",
)
replace_once(
    "crates/ripr/src/lsp/state.rs",
    """    pub(super) version: Option<i32>,
    pub(super) text: String,
    /// SHA-256 of the last known saved content: seeded from persisted bytes
""",
    """    pub(super) version: Option<i32>,
    pub(super) text: String,
    /// Whether `text` is a complete replay of the latest accepted document
    /// version. False after an invalid or ungrounded incremental sequence;
    /// full replacement text or a reopen can establish authority again.
    pub(super) buffer_text_current: bool,
    /// SHA-256 of the last known saved content: seeded from persisted bytes
""",
)
replace_once(
    "crates/ripr/src/lsp/state.rs",
    """            version,
            text,
            saved_digest,
""",
    """            version,
            text,
            buffer_text_current: true,
            saved_digest,
""",
)
replace_once(
    "crates/ripr/src/lsp/state.rs",
    """    pub(super) fn staleness_for_analyzed(
        &self,
        analyzed: Option<&String>,
    ) -> Option<DocumentStalenessReason> {
        match analyzed {
""",
    """    pub(super) fn staleness_for_analyzed(
        &self,
        analyzed: Option<&String>,
    ) -> Option<DocumentStalenessReason> {
        if !self.buffer_text_current {
            return Some(DocumentStalenessReason::BufferContentUnavailable);
        }
        match analyzed {
""",
)
replace_once(
    "crates/ripr/src/lsp/state.rs",
    """    pub(super) fn change(&mut self, params: DidChangeTextDocumentParams) -> QuarantineTransition {
        let uri = params.text_document.uri;
        let version = Some(params.text_document.version);
        let text = params
            .content_changes
            .into_iter()
            .last()
            .map(|change| change.text);
        if let Some(state) = self.documents.get_mut(&uri) {
            state.version = version;
            if let Some(text) = text {
                state.text = text;
            }
            return state.refresh_quarantine();
        }
        let Some(text) = text else {
            return QuarantineTransition::Unchanged;
        };
        let mut state = DocumentState::new(uri.clone(), version, text);
        let transition = state.refresh_quarantine();
        self.documents.insert(uri, state);
        transition
    }
""",
    """    /// Apply one ordered `textDocument/didChange` notification (#1625,
    /// RIPR-SPEC-0129). Versions must increase monotonically. Ranged changes
    /// are replayed against the result of the preceding change in the same
    /// notification using the negotiated position encoding. Any invalid or
    /// ungrounded range makes the buffer unavailable and therefore
    /// quarantined; a later full-content replacement can re-establish it.
    pub(super) fn change(
        &mut self,
        params: DidChangeTextDocumentParams,
        position_encoding: &PositionEncodingKind,
    ) -> QuarantineTransition {
        let uri = params.text_document.uri;
        let version = params.text_document.version;
        let changes = params.content_changes;
        if let Some(state) = self.documents.get_mut(&uri) {
            if state
                .version
                .is_some_and(|current_version| version <= current_version)
            {
                return QuarantineTransition::Unchanged;
            }
            state.version = Some(version);
            state.buffer_text_current = apply_document_changes(
                &mut state.text,
                state.buffer_text_current,
                changes,
                position_encoding,
            );
            return state.refresh_quarantine();
        }

        // A didChange without didOpen has no range base. A full replacement
        // may still establish current text; ranged deltas fail closed.
        let mut state = DocumentState::new(uri.clone(), Some(version), String::new());
        state.buffer_text_current =
            apply_document_changes(&mut state.text, false, changes, position_encoding);
        let transition = state.refresh_quarantine();
        self.documents.insert(uri, state);
        transition
    }
""",
)
replace_once(
    "crates/ripr/src/lsp/state.rs",
    """        if let Some(text) = text {
            state.text = text;
        }
        state.refresh_quarantine()
""",
    """        if let Some(text) = text {
            state.text = text;
            state.buffer_text_current = true;
        }
        state.refresh_quarantine()
""",
)
replace_once(
    "crates/ripr/src/lsp/state.rs",
    """fn document_path(uri: &Uri) -> PathBuf {
""",
    """fn apply_document_changes(
    text: &mut String,
    mut buffer_text_current: bool,
    changes: Vec<TextDocumentContentChangeEvent>,
    position_encoding: &PositionEncodingKind,
) -> bool {
    for change in changes {
        let TextDocumentContentChangeEvent {
            range,
            text: replacement,
            ..
        } = change;
        match range {
            None => {
                *text = replacement;
                buffer_text_current = true;
            }
            Some(range) if buffer_text_current => {
                buffer_text_current =
                    apply_incremental_change(text, range, &replacement, position_encoding);
            }
            Some(_) => {
                // Keep consuming the ordered notification so a later full
                // replacement may recover, but never apply a ranged delta to
                // text whose current identity is unknown.
            }
        }
    }
    buffer_text_current
}

fn apply_incremental_change(
    text: &mut String,
    range: Range,
    replacement: &str,
    position_encoding: &PositionEncodingKind,
) -> bool {
    let Some(start) = position_to_byte_offset(text, range.start, position_encoding) else {
        return false;
    };
    let Some(end) = position_to_byte_offset(text, range.end, position_encoding) else {
        return false;
    };
    if start > end {
        return false;
    }
    text.replace_range(start..end, replacement);
    true
}

fn position_to_byte_offset(
    text: &str,
    position: Position,
    position_encoding: &PositionEncodingKind,
) -> Option<usize> {
    let (line_start, line_end) = line_content_bounds(text, position.line)?;
    let line = text.get(line_start..line_end)?;
    let character_offset =
        encoded_character_byte_offset(line, position.character, position_encoding)?;
    line_start.checked_add(character_offset)
}

fn line_content_bounds(text: &str, target_line: u32) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut current_line = 0_u32;
    let mut line_start = 0_usize;
    let mut index = 0_usize;
    loop {
        if current_line == target_line {
            let mut line_end = line_start;
            while line_end < bytes.len() && !matches!(bytes[line_end], b'\r' | b'\n') {
                line_end += 1;
            }
            return Some((line_start, line_end));
        }
        if index >= bytes.len() {
            return None;
        }
        match bytes[index] {
            b'\r' => {
                index += 1;
                if index < bytes.len() && bytes[index] == b'\n' {
                    index += 1;
                }
                current_line = current_line.checked_add(1)?;
                line_start = index;
            }
            b'\n' => {
                index += 1;
                current_line = current_line.checked_add(1)?;
                line_start = index;
            }
            _ => index += 1,
        }
    }
}

fn encoded_character_byte_offset(
    line: &str,
    character: u32,
    position_encoding: &PositionEncodingKind,
) -> Option<usize> {
    if position_encoding == &PositionEncodingKind::UTF8 {
        let offset = usize::try_from(character).ok()?;
        return (offset <= line.len() && line.is_char_boundary(offset)).then_some(offset);
    }

    let uses_utf32 = position_encoding == &PositionEncodingKind::UTF32;
    let mut consumed = 0_u32;
    for (byte_offset, value) in line.char_indices() {
        if consumed == character {
            return Some(byte_offset);
        }
        let units = if uses_utf32 {
            1
        } else {
            match value.len_utf16() {
                1 => 1,
                2 => 2,
                _ => return None,
            }
        };
        consumed = consumed.checked_add(units)?;
        if consumed > character {
            return None;
        }
    }
    (consumed == character).then_some(line.len())
}

fn document_path(uri: &Uri) -> PathBuf {
""",
)

# Keep didSave from hashing stale reconstructed text after a failed delta.
replace_once(
    "crates/ripr/src/lsp/backend.rs",
    """    fn change_document(
        &self,
        params: DidChangeTextDocumentParams,
    ) -> Option<(Uri, QuarantineTransition)> {
        let uri = params.text_document.uri.clone();
        self.documents
            .lock()
            .ok()
            .map(|mut documents| (uri, documents.change(params)))
    }
""",
    """    fn change_document(
        &self,
        params: DidChangeTextDocumentParams,
    ) -> Option<(Uri, QuarantineTransition)> {
        let uri = params.text_document.uri.clone();
        let position_encoding = self
            .analysis_config()
            .map(|config| config.position_encoding)
            .unwrap_or(tower_lsp_server::ls_types::PositionEncodingKind::UTF16);
        self.documents
            .lock()
            .ok()
            .map(|mut documents| (uri, documents.change(params, &position_encoding)))
    }
""",
)
replace_once(
    "crates/ripr/src/lsp/backend.rs",
    """            .documents
            .get(uri)
            .map(|state| state.text.clone())
""",
    """            .documents
            .get(uri)
            .filter(|state| state.buffer_text_current)
            .map(|state| state.text.clone())
""",
)

# Focused document-store proofs.
replace_once(
    "crates/ripr/src/lsp/state.rs",
    """    use super::*;
    use tower_lsp_server::ls_types::{Position, Range};
""",
    """    use super::*;
    use tower_lsp_server::ls_types::VersionedTextDocumentIdentifier;
""",
)
replace_once(
    "crates/ripr/src/lsp/state.rs",
    """            version: Some(1),
            text: saved_text.to_string(),
            saved_digest: Some(digest_of(saved_text)),
""",
    """            version: Some(1),
            text: saved_text.to_string(),
            buffer_text_current: true,
            saved_digest: Some(digest_of(saved_text)),
""",
)
replace_once(
    "crates/ripr/src/lsp/state.rs",
    """    #[test]
    fn analysis_failure_kinds_keep_stable_wire_names() {
""",
    """    fn change_params(
        uri: &Uri,
        version: i32,
        content_changes: Vec<TextDocumentContentChangeEvent>,
    ) -> DidChangeTextDocumentParams {
        DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: uri.clone(),
                version,
            },
            content_changes,
        }
    }

    fn ranged_change(
        start: (u32, u32),
        end: (u32, u32),
        text: &str,
    ) -> TextDocumentContentChangeEvent {
        TextDocumentContentChangeEvent {
            range: Some(Range {
                start: Position {
                    line: start.0,
                    character: start.1,
                },
                end: Position {
                    line: end.0,
                    character: end.1,
                },
            }),
            range_length: None,
            text: text.to_string(),
        }
    }

    fn full_change(text: &str) -> TextDocumentContentChangeEvent {
        TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: text.to_string(),
        }
    }

    #[test]
    fn incremental_changes_replay_in_order_with_utf16_and_crlf() -> Result<(), String> {
        let uri = test_uri("file:///workspace/src/lib.rs")?;
        let mut store = DocumentStore::default();
        store.documents.insert(
            uri.clone(),
            clean_document_state(&uri, "alpha\r\n😀x"),
        );

        let transition = store.change(
            change_params(
                &uri,
                2,
                vec![
                    ranged_change((1, 2), (1, 3), "y"),
                    ranged_change((1, 3), (1, 3), "!"),
                ],
            ),
            &PositionEncodingKind::UTF16,
        );
        if transition != QuarantineTransition::Entered {
            return Err("the first divergent incremental edit must enter quarantine".to_string());
        }
        let Some(state) = store.state_for_uri(&uri) else {
            return Err("missing changed document".to_string());
        };
        if state.text != "alpha\r\n😀y!" || !state.buffer_text_current {
            return Err(format!("ordered UTF-16 replay drifted: {:?}", state.text));
        }
        if state.version != Some(2) {
            return Err("the accepted document version was not recorded".to_string());
        }
        Ok(())
    }

    #[test]
    fn incremental_changes_honor_utf8_and_utf32_offsets() -> Result<(), String> {
        let utf8_uri = test_uri("file:///workspace/src/utf8.rs")?;
        let utf32_uri = test_uri("file:///workspace/src/utf32.rs")?;
        let mut store = DocumentStore::default();
        store.documents.insert(
            utf8_uri.clone(),
            clean_document_state(&utf8_uri, "éx"),
        );
        store.documents.insert(
            utf32_uri.clone(),
            clean_document_state(&utf32_uri, "é😀x"),
        );

        store.change(
            change_params(&utf8_uri, 2, vec![ranged_change((0, 2), (0, 3), "y")]),
            &PositionEncodingKind::UTF8,
        );
        store.change(
            change_params(&utf32_uri, 2, vec![ranged_change((0, 1), (0, 2), "z")]),
            &PositionEncodingKind::UTF32,
        );

        if store.state_for_uri(&utf8_uri).map(|state| state.text.as_str()) != Some("éy") {
            return Err("UTF-8 byte offsets were not replayed correctly".to_string());
        }
        if store.state_for_uri(&utf32_uri).map(|state| state.text.as_str()) != Some("ézx") {
            return Err("UTF-32 scalar offsets were not replayed correctly".to_string());
        }
        Ok(())
    }

    #[test]
    fn invalid_incremental_range_fails_closed_until_full_replacement() -> Result<(), String> {
        let uri = test_uri("file:///workspace/src/lib.rs")?;
        let mut store = DocumentStore::default();
        store
            .documents
            .insert(uri.clone(), clean_document_state(&uri, "😀x"));

        let transition = store.change(
            change_params(&uri, 2, vec![ranged_change((0, 1), (0, 1), "?")]),
            &PositionEncodingKind::UTF16,
        );
        if transition != QuarantineTransition::Entered {
            return Err("a range inside a UTF-16 surrogate must fail closed".to_string());
        }
        let Some(state) = store.state_for_uri_mut(&uri) else {
            return Err("missing failed document state".to_string());
        };
        if state.buffer_text_current {
            return Err("invalid replay must revoke buffer-text authority".to_string());
        }
        let Some(quarantine) = state.quarantine.as_mut() else {
            return Err("invalid replay must quarantine the document".to_string());
        };
        if quarantine.reason != DocumentStalenessReason::BufferContentUnavailable {
            return Err("invalid replay exposed the wrong typed reason".to_string());
        }
        quarantine.withdrawal_disclosed = true;

        let transition = store.change(
            change_params(&uri, 3, vec![ranged_change((0, 0), (0, 0), "ignored")]),
            &PositionEncodingKind::UTF16,
        );
        if transition != QuarantineTransition::Unchanged {
            return Err("ranged edits cannot mutate unavailable text".to_string());
        }
        let transition = store.change(
            change_params(&uri, 4, vec![full_change("😀x")]),
            &PositionEncodingKind::UTF16,
        );
        if transition
            != (QuarantineTransition::Exited {
                was_disclosed: true,
            })
        {
            return Err("a later full replacement must recover the quarantine episode".to_string());
        }
        let Some(state) = store.state_for_uri(&uri) else {
            return Err("missing recovered document state".to_string());
        };
        if !state.buffer_text_current || state.text != "😀x" {
            return Err("full replacement did not restore current text".to_string());
        }
        Ok(())
    }

    #[test]
    fn stale_or_duplicate_document_versions_are_ignored() -> Result<(), String> {
        let uri = test_uri("file:///workspace/src/lib.rs")?;
        let mut store = DocumentStore::default();
        let mut state = clean_document_state(&uri, "fn saved() {}");
        state.version = Some(7);
        store.documents.insert(uri.clone(), state);

        let transition = store.change(
            change_params(&uri, 7, vec![full_change("fn stale() {}")]),
            &PositionEncodingKind::UTF16,
        );
        if transition != QuarantineTransition::Unchanged {
            return Err("a duplicate document version must be ignored".to_string());
        }
        let Some(state) = store.state_for_uri(&uri) else {
            return Err("missing document after duplicate version".to_string());
        };
        if state.text != "fn saved() {}" || state.version != Some(7) || state.is_quarantined() {
            return Err("duplicate version mutated authoritative document state".to_string());
        }
        Ok(())
    }

    #[test]
    fn analysis_failure_kinds_keep_stable_wire_names() {
""",
)

# Ratify the bounded wire/state slice without claiming the remaining lifecycle work.
spec_path = Path("docs/specs/RIPR-SPEC-0129-editor-integration-contract.md")
spec_text = spec_path.read_text(encoding="utf-8")
heading = "## Incremental saved-workspace synchronization"
if heading not in spec_text:
    spec_text = spec_text.rstrip() + r"""

## Incremental saved-workspace synchronization

The standard editor layer advertises explicit `TextDocumentSyncOptions`:
`openClose=true`, `change=Incremental`, `save.includeText=false`,
`willSave=false`, and `willSaveWaitUntil=false`. The server retains one
in-memory text and monotonically increasing version per open document and
replays each `didChange.contentChanges` entry in order using the negotiated
UTF-8, UTF-16, or UTF-32 position encoding.

This state is transport authority only. Unsaved text is never analyzed or
written into analysis caches, snapshots, receipts, or repair packets. The
first clean-to-dirty transition withdraws saved-state line-local diagnostics
through the existing document quarantine. An invalid range or an incremental
change without a grounded full buffer fails closed as
`buffer_content_unavailable`; later ranged changes are ignored until a full
replacement or reopen restores current text.

This slice does not claim completion of save-order confirmation, rename
identity, reconnect restoration, or multi-client document ownership. Those
remain explicit lifecycle work under issue #1625.
""" + "\n"
    spec_path.write_text(spec_text, encoding="utf-8")

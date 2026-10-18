<!-- section: Fixed -->
- The language server no longer parks one request handler per `textDocument/didOpen` for the whole 200ms refresh debounce. Opening many files at once previously throttled input to about 50ms per open and emitted a status notification burst per open; concurrent interactive triggers now coalesce into the open debounce window and the server keeps reading input (#8528).

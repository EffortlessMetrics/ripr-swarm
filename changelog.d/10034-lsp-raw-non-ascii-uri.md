<!-- section: Fixed -->
- The editor server starts when a client sends a raw non-ASCII `rootUri` or `workspaceFolders[].uri` in `initialize` (for example `file:///tmp/café-repo`). The server percent-encodes those bytes on the first frame instead of failing the whole request with an `unexpected character` error; already-encoded URIs pass through unchanged (#10016, #10034).

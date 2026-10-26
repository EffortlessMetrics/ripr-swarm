<!-- section: Fixed -->
- Editor sessions no longer fail during `initialize` solely because `rootUri` or `workspaceFolders[].uri` contains raw non-ASCII characters, such as `file:///tmp/café-repo`. The server percent-encodes those bytes in the first frame before URI parsing; already-encoded URIs pass through unchanged (#10016, #10034).

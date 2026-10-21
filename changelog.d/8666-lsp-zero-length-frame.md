<!-- section: Fixed -->
- `ripr lsp --stdio` no longer delays the reply to a request that shares a read with a preceding `Content-Length: 0` frame; the transport now ends the read at an empty frame so the codec drains and picks up the next frame immediately (#8666).

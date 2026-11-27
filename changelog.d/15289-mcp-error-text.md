<!-- section: Fixed -->
- MCP errors say what to send: a pre-initialize request names the `initialize`
  lifecycle, a malformed `initialize` names the bad field (also after a
  handshake), `tools/call` and `resources/read` param errors (including
  non-object params) say what to pass, a tool `snapshot_id` outside
  `snapshot:sha256:<64 hex>` is `invalid_argument` naming that form ahead of
  the echoed value, every echoed argument is
  escaped by one rule, and the oversize-message error states its byte limit
  (#15289).

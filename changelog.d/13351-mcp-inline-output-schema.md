<!-- section: Fixed -->
- MCP's `ripr_prepare_repair` output schema admits the already-emitted nullable
  inline test module boundary, so validating clients can accept successful
  preparation and replay packets without loosening the packet contract (#13351).

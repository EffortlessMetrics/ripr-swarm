<!-- section: Changed -->
- MCP: resource payloads (`ripr://workspace/status` and every resource template) now serialize their document text compact, the same encoding tool results already use, instead of pretty-printing it; the structured document is unchanged and tool/resource equivalence (#7145) stays pinned over the parsed document. This is an output-contract change for clients that read the raw resource text bytes; parsed-JSON consumers are unaffected.
<!-- section: Removed -->
- MCP: the `ripr_get_gap` document and the `ripr://gap/{canonical_id}` resource no longer carry the internal provenance fields `readiness.prepared_by`, `readiness.evaluation_basis`, and `repair_boundary.prepared_by`; no documented consumer read them and the structured evidence is otherwise identical.

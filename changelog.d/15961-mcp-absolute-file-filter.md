<!-- section: Fixed -->
- MCP `ripr_list_gaps` accepts an absolute `file` path under the workspace root (it is applied as the root-relative path) and refuses any other absolute path with the form to send, instead of returning an empty page that read as "no gaps in that file". A `null` sent for an optional string or boolean argument is now refused with the instruction to omit it (#15961).

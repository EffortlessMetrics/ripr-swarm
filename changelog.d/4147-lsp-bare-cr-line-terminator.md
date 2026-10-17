<!-- section: Fixed -->
- Incremental document sync now treats a bare CR as a line terminator, as the protocol and clients do. A buffer using CR line endings, or ending in a CR, no longer turns a valid edit into a refused change that withdraws the document diagnostics (#4147).

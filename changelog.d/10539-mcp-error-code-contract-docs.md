<!-- section: Docs -->
- The MCP interop guide now states that the JSON-RPC error code is coarse by contract, tells consumers to branch on `error.data.code` and follow `error.data.recovery`, and tabulates every kind with its `resources/read` code; a test keeps the table equal to the failure vocabulary and `resource_failure` mapping (#10539, #10518).

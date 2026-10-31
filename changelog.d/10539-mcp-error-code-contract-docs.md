<!-- section: Docs -->
- The MCP interop guide now states that the JSON-RPC error code is coarse by contract, tells consumers to branch on `error.data.code` and follow `error.data.recovery`, and tabulates every kind with its `resources/read` code; a test pins the table's kind set to the failure vocabulary and each kind's JSON-RPC code to the `resource_failure` mapping (#10539, #10518).

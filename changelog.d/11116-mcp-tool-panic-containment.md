<!-- section: Fixed -->
- A panic inside any MCP tool handler other than `ripr_refresh` is now contained: the call answers a typed `analysis_failed` tool failure naming the tool and a next step, and the `ripr mcp` stdio session keeps serving instead of ending (#11116).

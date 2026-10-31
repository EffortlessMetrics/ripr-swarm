<!-- section: Fixed -->
- A panic inside an LSP request or notification handler no longer ends the sidecar session; the request now answers with a JSON-RPC `InternalError` that names the method and a retry step, and later requests keep being served (#11116).

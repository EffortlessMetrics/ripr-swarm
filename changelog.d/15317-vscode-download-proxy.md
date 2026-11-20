<!-- section: Fixed -->
- The VS Code extension's managed server download now honors VS Code
  `http.proxy`, `HTTPS_PROXY`/`https_proxy` and `NO_PROXY`/`no_proxy`
  (`http://` proxies with optional basic auth, redirects included; credentials
  are never logged). Unsupported proxy schemes fail closed naming
  `ripr.server.path`, and the network-failure hint now names the proxy
  settings (#15317).

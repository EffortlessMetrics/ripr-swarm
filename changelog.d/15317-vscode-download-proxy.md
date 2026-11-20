<!-- section: Fixed -->
- The VS Code extension's network-failure hint for the managed server download
  now names the setting that fixes a proxy problem: the download uses VS Code's
  proxy support, so set `http.proxy` (with `http.noProxy` for direct hosts) and
  keep `http.proxySupport` on. The extension README and server provisioning
  docs say so, and extension-host tests pin that the download goes through
  `http.proxy` (#15317).

<!-- section: Fixed -->
- The VS Code extension's network-failure hint for the managed server download
  no longer says "check the proxy" with nothing to configure. It names VS Code's
  proxy support (`http.proxy`, `HTTPS_PROXY`, the system proxy, unless
  `http.proxySupport` is off), and the README and provisioning docs describe how
  the download is proxied (#15317).

<!-- section: Fixed -->
- VS Code: a `ripr.server.downloadBaseUrl` that is not a plain `https://` folder address
  (not a URL, another scheme, a query or fragment, or the manifest file itself) now skips
  the managed download, and the unavailable-server message names the setting, quotes the
  value, says what is wrong and asks for the folder address or an empty value, instead of
  a bare "Invalid URL" (#15163).

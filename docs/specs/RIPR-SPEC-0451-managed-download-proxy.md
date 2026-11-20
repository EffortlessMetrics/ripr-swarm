# RIPR-SPEC-0451: Managed Server Download Through a Proxy

Status: accepted

Owner: product / swarm

Created: 2026-11-20

Linked issues:

- [#15317](https://github.com/EffortlessMetrics/ripr-swarm/issues/15317) - the
  VS Code managed server download ignored `http.proxy`, `HTTPS_PROXY` and
  `NO_PROXY`, so it could not reach its host behind a corporate proxy.

## Problem

The VS Code extension downloads its `ripr` server binary with Node's `https`
module directly. Behind a proxy that is the only route out, that request never
reaches its host, and the developer sees a network failure even though VS Code
itself is configured with `http.proxy`. The extension must route the download
through the configured proxy, keep every existing download guarantee, and fail
closed with a workaround when the proxy is a kind it cannot use.

## Behavior

Proxy selection, decided again for every redirect hop:

- The proxy comes from the first nonblank source in this order: VS Code
  `http.proxy`, `HTTPS_PROXY`, `https_proxy`. With none set, the request goes
  direct.
- `NO_PROXY` (or `no_proxy` when `NO_PROXY` is blank) lists hosts that go
  direct: exact hosts, `.suffix` and `*.suffix` domain suffixes, an optional
  `:port`, exact IP literals, and `*` for everything. Matching is
  case-insensitive. CIDR ranges are not supported and do not exempt a host.
- The extension always adds `localhost`, `127.0.0.1` and `::1` to the
  `NO_PROXY` list it reads, so loopback always goes direct.
- A bare `host:port` proxy value is read as `http://host:port`.

Transport:

- Only `http://` proxies are supported. The extension opens an HTTP `CONNECT`
  tunnel to the target host and port, then runs TLS to the real host through
  it, with the target host as the TLS server name and normal certificate and
  hostname verification.
- Userinfo in the proxy URL (`http://user:password@host:port`, or a
  password-only `http://:token@host:port`) is percent-decoded and sent only as
  a `Proxy-Authorization: Basic` header on the `CONNECT` request. The origin
  never receives it, and no message or log line renders it.
- A non-200 `CONNECT` answer, a tunnel socket error or a TLS setup failure
  fails the download as a network-class failure naming the proxy without its
  credentials. It never falls back to a direct connection.

Fail-closed rules:

- A `socks://`, `socks5://`, `https://` or otherwise unsupported proxy scheme,
  an unparseable proxy URL, or userinfo that is not valid percent-encoding
  fails the download before any connection, with a message naming the
  workaround: set `ripr.server.path` to a local binary or install `ripr` on
  `PATH`.
- The existing size limits, fetch timeout, redirect host allowlist and
  SHA-256 checksum checks apply unchanged on the proxied path.

## Required Evidence

- Unit tests for source precedence, `NO_PROXY` matching (positive and
  negative), credential decoding and redaction, and each unsupported shape.
- Loopback integration tests through a real local `CONNECT` proxy and TLS
  origin: a tunnelled request including a redirect hop, `NO_PROXY` bypass, a
  refused `CONNECT`, an untrusted origin certificate, and a certificate whose
  SAN does not cover the requested host.
- `tsc -p .` clean for the extension, and the hosted `vscode` job.

## Non-Goals

- SOCKS or `https://` proxies, PAC files, NTLM or Kerberos proxy auth.
- `http.proxyStrictSSL` or `http.proxySupport`: certificate checks are never
  relaxed by proxy settings.
- Proxy support for anything other than the managed server download.
- How the unavailable-server state is explained (RIPR-SPEC-0049) and archive
  inspection (RIPR-SPEC-0437).

## Acceptance Examples

1. With `http.proxy` set to `http://proxy:3128` and `HTTPS_PROXY` set to
   another proxy, the download tunnels through `proxy:3128`.
2. With `NO_PROXY=.corp.example` the download of a host under
   `corp.example` goes direct and the proxy sees no `CONNECT`.
3. A download that redirects is tunnelled on both hops, and the origin sees
   no `Proxy-Authorization` header on either.
4. A proxy that answers `CONNECT` with HTTP 407 fails the download with a
   message naming the proxy and HTTP 407, without its password, and no direct
   request is made.
5. `http.proxy` set to `socks5://proxy:1080` fails before any connection with a
   message naming `ripr.server.path`.
6. An origin whose certificate is not trusted, or does not cover the
   requested host, fails through the tunnel and receives no request.

## Test Mapping

- `editors/vscode/test/suite/download_proxy.test.ts` - precedence, `NO_PROXY`,
  credentials, unsupported shapes (examples 1, 2, 5) and the loopback
  `CONNECT` proxy suite (examples 3, 4, 6).

## Implementation Mapping

- `editors/vscode/src/downloadProxy.ts::resolveProxy` - source precedence,
  `NO_PROXY` and fail-closed proxy parsing.
- `editors/vscode/src/downloadProxy.ts::ProxyTunnelAgent` - `CONNECT` tunnel
  and TLS to the target host.
- `editors/vscode/src/downloader.ts::fetchManifestBytesOverHttps` - per-hop
  proxy resolution inside the existing limits.

## Metrics

- No new metric. Proxy failures surface through the existing managed-install
  failure channels.

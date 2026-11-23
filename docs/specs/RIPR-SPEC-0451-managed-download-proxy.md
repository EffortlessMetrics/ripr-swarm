# RIPR-SPEC-0451: Managed Server Download Through a Proxy

Status: accepted

Owner: product / swarm

Created: 2026-11-20

Linked issues:

- [#15317](https://github.com/EffortlessMetrics/ripr-swarm/issues/15317) - the
  VS Code managed server download was reported to ignore `http.proxy` and
  `HTTPS_PROXY`, with a failure hint that named no configurable fix.

## Problem

The managed server download looked as if it ignored proxies, because it calls
Node's `https` module directly. In the VS Code extension host that module is
patched by VS Code's proxy support (`http.proxySupport`, default `override`),
which routes the request through `http.proxy` and replaces any custom agent
for non-localhost hosts. A second, extension-owned proxy client would be
bypassed by that patch and could disagree with the user's VS Code settings.
What was missing was proof that the route works, and a failure hint that names
the settings that control it.

## Behavior

- The managed download (manifest and archive) is made with Node's `https`
  module from the extension host and adds no proxy client of its own. It goes
  through the proxy VS Code resolves for it: `http.proxy`, or the environment
  proxy when that is unset.
- A host listed in `http.noProxy` goes direct.
- A proxy that refuses the tunnel reaches the download as that HTTP status
  (for example 407), not as a silent direct retry.
- When the download got no answer from its host, the network-failure hint
  names `http.proxy` and `http.proxySupport`, plus the offline route
  (`ripr.server.path`, `ripr.server.downloadBaseUrl`).
- Proxy routing does not relax the redirect allowlist, size limits or the
  archive checksum.

## Required Evidence

- Extension-host tests against a local `CONNECT` proxy: with `http.proxy` set,
  the proxy sees the download's `CONNECT` for its host; with that host in
  `http.noProxy`, it does not.
- A test that the network-failure hint names the proxy settings.
- `tsc -p .` clean for the extension, and the hosted `vscode` job.

## Non-Goals

- An extension-owned proxy client, SOCKS support, or proxy auth beyond what
  VS Code provides.
- Proxy support when the user sets `http.proxySupport` to `off`.
- How the unavailable-server state is explained in general (RIPR-SPEC-0049)
  and archive inspection (RIPR-SPEC-0437).

## Acceptance Examples

1. With `http.proxy` set to a local proxy, a manifest fetch for
   `ripr-proxy-probe.invalid` makes the proxy see `CONNECT
   ripr-proxy-probe.invalid:443`, and a 407 answer comes back as an HTTP 407
   download failure.
2. With `ripr-proxy-probe.invalid` in `http.noProxy`, the same fetch never
   reaches the proxy.
3. A download that never reached its host shows a hint naming `http.proxy`,
   `http.proxySupport` and `ripr.server.path`.

## Test Mapping

- `editors/vscode/test/suite/download_proxy.test.ts` - examples 1, 2 and 3 in
  the extension host.
- `editors/vscode/test/suite/missing_server_message.test.ts` - the hint in the
  missing-server guidance.

## Implementation Mapping

- `editors/vscode/src/downloader.ts::fetchManifestBytesOverHttps` - the
  host-patched `https` request the proxy applies to.
- `editors/vscode/src/serverResolver.ts::NETWORK_FAILURE_REMEDY` - the
  proxy-aware recovery hint.

## Metrics

- No new metric. Proxy failures surface through the existing managed-install
  failure channels.

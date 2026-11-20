import * as http from 'http';
import * as https from 'https';
import * as net from 'net';
import * as tls from 'tls';
import type { Duplex } from 'stream';

/**
 * Proxy support for the managed server download (#15317). The download uses
 * Node's raw `https` client, which ignores VS Code's `http.proxy` and the
 * `HTTPS_PROXY` family. Resolution here is a pure function of injected
 * settings so precedence, `NO_PROXY` and redaction are unit-testable; the
 * tunnel uses only Node built-ins (an `http` CONNECT to the proxy, then TLS to
 * the real host over that socket), so no runtime dependency is added.
 */

export interface ProxySettings {
  /** VS Code `http.proxy`; empty or undefined means unset. */
  readonly httpProxy?: string;
  /** Process environment; only the proxy variables are read. */
  readonly env: Readonly<Record<string, string | undefined>>;
}

export interface ResolvedProxy {
  readonly kind: 'proxy';
  readonly host: string;
  readonly port: number;
  /** Decoded `user:password`, never rendered. */
  readonly auth?: string;
  /** Credential-free text safe for logs and messages. */
  readonly display: string;
  readonly source: 'http.proxy' | 'HTTPS_PROXY' | 'https_proxy';
}

export type ProxyResolution =
  | { readonly kind: 'direct' }
  | ResolvedProxy
  | { readonly kind: 'unsupported'; readonly message: string };

export const PROXY_WORKAROUND =
  'Set ripr.server.path to a ripr binary already on this machine (or install ripr on PATH) to skip the download.';

/** Replace any `user:password@` userinfo in `text` with `***@`. */
export function redactProxyCredentials(text: string): string {
  return text.replace(/([a-z][a-z0-9+.-]*:\/\/)[^\s/?#@]*@/gi, '$1***@');
}

/** Hosts that never go through a proxy unless the user's settings say otherwise. */
export const IMPLICIT_NO_PROXY = 'localhost,127.0.0.1,::1';

/** The ambient settings: `http.proxy` plus the process environment. */
export function ambientNoProxy(env: Readonly<Record<string, string | undefined>>): string {
  const configured = firstNonblank(env.NO_PROXY, env.no_proxy) ?? '';
  return configured.length > 0 ? `${configured},${IMPLICIT_NO_PROXY}` : IMPLICIT_NO_PROXY;
}

function firstNonblank(...values: ReadonlyArray<string | undefined>): string | undefined {
  return values.find((value) => value !== undefined && value.trim().length > 0)?.trim();
}

/**
 * True when `NO_PROXY` exempts `host:port`. Entries are comma-separated; `*`
 * exempts everything; `example.com`, `.example.com` and `*.example.com` match
 * the domain and its subdomains on a label boundary; an optional `:port`
 * narrows the entry; IP literals match exactly.
 */
export function noProxyMatches(noProxy: string | undefined, host: string, port: number): boolean {
  if (!noProxy) {
    return false;
  }
  const target = canonicalIp(host.toLowerCase().replace(/^\[|\]$/g, ''));
  for (const raw of noProxy.split(/[,\s]+/)) {
    let entry = raw.trim().toLowerCase();
    if (entry.length === 0) {
      continue;
    }
    if (entry === '*') {
      return true;
    }
    let entryPort: number | undefined;
    const portMatch = /^(.*):(\d+)$/.exec(entry);
    if (portMatch && !(portMatch[1].includes(':') && !portMatch[1].startsWith('['))) {
      entry = portMatch[1];
      entryPort = Number(portMatch[2]);
    }
    entry = entry.replace(/^\[|\]$/g, '').replace(/^\*?\./, (m) => (m.endsWith('.') ? '.' : m));
    if (entryPort !== undefined && entryPort !== port) {
      continue;
    }
    // An IP literal matches only the whole address, never a label suffix
    // (`0.1` must not exempt `10.0.0.1`); CIDR ranges are not supported.
    if (net.isIP(target) !== 0) {
      if (target === canonicalIp(entry)) {
        return true;
      }
      continue;
    }
    if (entry.startsWith('.')) {
      if (target.endsWith(entry) || target === entry.slice(1)) {
        return true;
      }
      continue;
    }
    if (target === entry || target.endsWith(`.${entry}`)) {
      return true;
    }
  }
  return false;
}

/**
 * The WHATWG URL spelling of an IPv6 literal (`2001:db8:0:0::1` becomes
 * `2001:db8::1`, `::ffff:192.0.2.1` becomes `::ffff:c000:201`), so a
 * `NO_PROXY` entry matches the hostname the URL parser produced. Any other
 * text is returned unchanged.
 */
function canonicalIp(text: string): string {
  if (net.isIPv6(text)) {
    try {
      return new URL(`http://[${text}]/`).hostname.replace(/^\[|\]$/g, '');
    } catch {
      return text;
    }
  }
  return text;
}

/**
 * Choose how to reach `targetUrl`: VS Code `http.proxy`, then `HTTPS_PROXY`,
 * then `https_proxy`; `NO_PROXY`/`no_proxy` exempts a host. Only `http://`
 * proxies (optionally with basic-auth userinfo) are supported; any other
 * scheme resolves to `unsupported` so the download fails closed with the
 * workaround instead of silently going direct.
 */
export function resolveProxy(targetUrl: string, settings: ProxySettings): ProxyResolution {
  let target: URL;
  try {
    target = new URL(targetUrl);
  } catch {
    return { kind: 'direct' };
  }
  const { env } = settings;
  const fromSettings = firstNonblank(settings.httpProxy);
  const fromUpper = firstNonblank(env.HTTPS_PROXY);
  const fromLower = firstNonblank(env.https_proxy);
  const source = fromSettings ? 'http.proxy' : fromUpper ? 'HTTPS_PROXY' : fromLower ? 'https_proxy' : undefined;
  const raw = fromSettings ?? fromUpper ?? fromLower;
  if (!source || !raw) {
    return { kind: 'direct' };
  }
  const port = target.port ? Number(target.port) : target.protocol === 'https:' ? 443 : 80;
  if (noProxyMatches(firstNonblank(env.NO_PROXY, env.no_proxy), target.hostname, port)) {
    return { kind: 'direct' };
  }
  // A bare `host:port` is the common env spelling; read it as an http proxy.
  const withScheme = /^[a-z][a-z0-9+.-]*:\/\//i.test(raw) ? raw : `http://${raw}`;
  let proxy: URL;
  try {
    proxy = new URL(withScheme);
  } catch {
    return {
      kind: 'unsupported',
      message: `The proxy from ${source} is not a valid URL. Use http://host:port, or unset it. ${PROXY_WORKAROUND}`
    };
  }
  const display = `${proxy.protocol}//${proxy.host}`;
  if (proxy.protocol !== 'http:') {
    const scheme = proxy.protocol.replace(/:$/, '');
    return {
      kind: 'unsupported',
      message:
        `The proxy from ${source} (${display}) uses ${scheme}://, which the managed server download does not support; only http:// proxies are supported. ` +
        PROXY_WORKAROUND
    };
  }
  let auth: string | undefined;
  try {
    auth = proxy.username || proxy.password
      ? `${decodeURIComponent(proxy.username)}:${decodeURIComponent(proxy.password)}`
      : undefined;
  } catch {
    return {
      kind: 'unsupported',
      message: `The proxy credentials from ${source} (${display}) are not valid percent-encoding; encode special characters such as % as %25. ${PROXY_WORKAROUND}`
    };
  }
  return {
    kind: 'proxy',
    host: proxy.hostname.replace(/^\[|\]$/g, ''),
    port: proxy.port ? Number(proxy.port) : 80,
    ...(auth === undefined ? {} : { auth }),
    display,
    source
  };
}

/** Open a CONNECT tunnel to `host:port` through `proxy`. */
export function openProxyTunnel(
  proxy: ResolvedProxy,
  host: string,
  port: number,
  timeoutMs: number
): Promise<net.Socket> {
  return new Promise((resolve, reject) => {
    const authority = host.includes(':') ? `[${host}]:${port}` : `${host}:${port}`;
    const headers: Record<string, string> = { host: authority };
    if (proxy.auth !== undefined) {
      headers['proxy-authorization'] = `Basic ${Buffer.from(proxy.auth).toString('base64')}`;
    }
    const request = http.request({
      host: proxy.host,
      port: proxy.port,
      method: 'CONNECT',
      path: authority,
      headers,
      agent: false
    });
    const fail = (message: string, cause?: NodeJS.ErrnoException): void => {
      request.destroy();
      const error = new Error(`Proxy tunnel failed: ${message}`) as NodeJS.ErrnoException;
      if (cause?.code) {
        error.code = cause.code;
      }
      reject(error);
    };
    request.setTimeout(timeoutMs, () => fail(`timed out after ${timeoutMs} ms reaching proxy ${proxy.display}.`));
    request.on('connect', (response, socket) => {
      if (response.statusCode === 200) {
        request.setTimeout(0);
        resolve(socket);
        return;
      }
      socket.destroy();
      const hint = response.statusCode === 407 ? ' (proxy authentication required; put credentials in the proxy URL)' : '';
      fail(`proxy ${proxy.display} refused CONNECT to ${authority} with HTTP ${response.statusCode}${hint}.`);
    });
    request.on('error', (error: NodeJS.ErrnoException) => {
      fail(`could not reach proxy ${proxy.display} (${error.code ?? error.message}).`, error);
    });
    request.end();
  });
}

/**
 * An `https.Agent` that reaches every host through `proxy`: CONNECT, then TLS
 * to the real host over the tunnel with normal certificate verification (the
 * proxy only ever sees ciphertext).
 */
export class ProxyTunnelAgent extends https.Agent {
  constructor(
    private readonly proxy: ResolvedProxy,
    private readonly connectTimeoutMs = 30_000,
    private readonly tlsOverrides: tls.ConnectionOptions = {}
  ) {
    super();
  }

  // Node's Agent accepts an asynchronous createConnection that reports through
  // the callback; the @types signature only models the synchronous form.
  override createConnection(
    options: https.RequestOptions,
    callback?: (error: Error | null, stream: Duplex) => void
  ): Duplex | null | undefined {
    const host = String(options.host ?? options.hostname ?? '');
    const port = Number(options.port ?? 443);
    openProxyTunnel(this.proxy, host, port, this.connectTimeoutMs).then(
      (tunnel) => {
        try {
          const secure = tls.connect({
            ...(https.globalAgent.options as tls.ConnectionOptions),
            ...(options as tls.ConnectionOptions),
            ...this.tlsOverrides,
            socket: tunnel,
            servername: net.isIP(host) ? undefined : host
          });
          callback?.(null, secure);
        } catch (error) {
          // A synchronous TLS setup failure must not leak the tunnel or leave
          // the request pending.
          tunnel.destroy();
          callback?.(error instanceof Error ? error : new Error(String(error)), undefined as unknown as Duplex);
        }
      },
      (error: Error) => callback?.(error, undefined as unknown as Duplex)
    );
    // The real socket arrives through the callback; the placeholder is unused.
    return undefined;
  }
}

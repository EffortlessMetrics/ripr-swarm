"use strict";

// Isolated loopback npm registry for no-publish package qualification (#4627,
// #4632). It serves only the tarballs it was staged with, has no upstream,
// proxy or write route, binds 127.0.0.1 on an ephemeral port, and records
// every request so a journey can prove which packages npm actually fetched.

const crypto = require("node:crypto");
const fs = require("node:fs");
const http = require("node:http");
const path = require("node:path");
const zlib = require("node:zlib");

const MAX_TARBALL_BYTES = 256 * 1024 * 1024;
const MAX_MANIFEST_BYTES = 1024 * 1024;
const MAX_LOGGED_REQUESTS = 512;

class RegistryError extends Error {
  constructor(code, message) {
    super(message);
    this.name = "RegistryError";
    this.code = code;
  }
}

function tarString(block, offset, length) {
  const slice = block.subarray(offset, offset + length);
  const end = slice.indexOf(0);
  return slice.subarray(0, end === -1 ? slice.length : end).toString("utf8");
}

function tarSize(block) {
  const text = tarString(block, 124, 12).trim();
  if (!/^[0-7]+$/.test(text)) {
    throw new RegistryError("tarball_invalid", `tar entry has a non-octal size field ${JSON.stringify(text)}`);
  }
  return Number.parseInt(text, 8);
}

// Lists every regular entry of a gzip tarball as { name, mode, bytes }. The
// reader accepts ustar/GNU headers with optional PAX names; anything else is
// refused rather than guessed.
function readTarball(bytes) {
  let tar;
  try {
    tar = zlib.gunzipSync(bytes, { maxOutputLength: MAX_TARBALL_BYTES });
  } catch (error) {
    throw new RegistryError("tarball_invalid", `tarball is not gzip data: ${error.message}`);
  }
  const entries = [];
  let offset = 0;
  let paxName = null;
  while (offset + 512 <= tar.length) {
    const header = tar.subarray(offset, offset + 512);
    if (header.every((byte) => byte === 0)) break;
    const size = tarSize(header);
    const type = String.fromCharCode(header[156] || 48);
    const prefix = tarString(header, 345, 155);
    const shortName = tarString(header, 0, 100);
    const name = paxName || (prefix ? `${prefix}/${shortName}` : shortName);
    const mode = Number.parseInt(tarString(header, 100, 8).trim() || "0", 8);
    const body = tar.subarray(offset + 512, offset + 512 + size);
    if (body.length !== size) {
      throw new RegistryError("tarball_invalid", `tar entry ${name} is truncated`);
    }
    paxName = null;
    if (type === "x") {
      const match = /(?:^|\n)\d+ path=([^\n]*)\n/.exec(body.toString("utf8"));
      paxName = match ? match[1] : null;
    } else if (type === "0" || type === "\0") {
      entries.push({ name, mode, bytes: Buffer.from(body) });
    } else if (type !== "5" && type !== "g") {
      throw new RegistryError("tarball_invalid", `tar entry ${name} has unsupported type ${JSON.stringify(type)}`);
    }
    offset += 512 + Math.ceil(size / 512) * 512;
  }
  return entries;
}

function packageManifest(entries, file) {
  const entry = entries.find((item) => item.name === "package/package.json");
  if (!entry) {
    throw new RegistryError("tarball_manifest_missing", `${file} has no package/package.json`);
  }
  if (entry.bytes.length > MAX_MANIFEST_BYTES) {
    throw new RegistryError("tarball_manifest_oversized", `${file} package.json exceeds ${MAX_MANIFEST_BYTES} bytes`);
  }
  let manifest;
  try {
    manifest = JSON.parse(entry.bytes.toString("utf8"));
  } catch (error) {
    throw new RegistryError("tarball_manifest_invalid", `${file} package.json is not JSON: ${error.message}`);
  }
  if (!manifest || typeof manifest !== "object" || Array.isArray(manifest) ||
      typeof manifest.name !== "string" || typeof manifest.version !== "string") {
    throw new RegistryError("tarball_manifest_invalid", `${file} package.json must name a package and version`);
  }
  if (manifest.scripts && Object.keys(manifest.scripts).some((key) => /^(pre|post)?(install|prepare)$/.test(key))) {
    throw new RegistryError("tarball_lifecycle_script", `${file} declares an install-time lifecycle script`);
  }
  return manifest;
}

// Reads one staged tarball into the identity the registry serves and the
// journey records: npm integrity (sha512), sha1 shasum and sha256 digest.
function stageTarball(file) {
  const stat = fs.statSync(file);
  if (!stat.isFile() || stat.size > MAX_TARBALL_BYTES) {
    throw new RegistryError("tarball_invalid", `${file} is not a bounded regular file`);
  }
  const bytes = fs.readFileSync(file);
  const entries = readTarball(bytes);
  const manifest = packageManifest(entries, path.basename(file));
  return {
    file: path.resolve(file),
    basename: path.basename(file),
    bytes,
    entries,
    manifest,
    sha256: `sha256:${crypto.createHash("sha256").update(bytes).digest("hex")}`,
    integrity: `sha512-${crypto.createHash("sha512").update(bytes).digest("base64")}`,
    shasum: crypto.createHash("sha1").update(bytes).digest("hex"),
  };
}

function packument(name, staged, origin) {
  const versions = {};
  for (const item of staged) {
    const { scripts: _scripts, ...manifest } = item.manifest;
    versions[item.manifest.version] = {
      ...manifest,
      _id: `${name}@${item.manifest.version}`,
      dist: {
        tarball: `${origin}/-/tarballs/${encodeURIComponent(item.basename)}`,
        integrity: item.integrity,
        shasum: item.shasum,
      },
    };
  }
  const latest = staged[staged.length - 1].manifest.version;
  return { name, "dist-tags": { latest }, versions };
}

// Starts the registry over `tarballs` (paths). Resolves to a handle with the
// origin URL, the staged identities, the request log and an idempotent close.
async function startRegistry(tarballs, options = {}) {
  if (!Array.isArray(tarballs) || tarballs.length === 0) {
    throw new RegistryError("registry_empty", "an isolated registry needs at least one staged tarball");
  }
  const staged = tarballs.map(stageTarball);
  const byName = new Map();
  const byFile = new Map();
  for (const item of staged) {
    const list = byName.get(item.manifest.name) || [];
    if (list.some((other) => other.manifest.version === item.manifest.version)) {
      throw new RegistryError("registry_duplicate", `${item.manifest.name}@${item.manifest.version} is staged twice`);
    }
    if (byFile.has(item.basename)) {
      throw new RegistryError("registry_duplicate", `tarball file name ${item.basename} is staged twice`);
    }
    list.push(item);
    byName.set(item.manifest.name, list);
    byFile.set(item.basename, item);
  }
  const requests = [];
  let dropped = 0;
  const record = (entry) => {
    if (requests.length < MAX_LOGGED_REQUESTS) {
      requests.push(entry);
      if (options.requestLog) fs.appendFileSync(options.requestLog, `${JSON.stringify(entry)}\n`);
    } else {
      dropped += 1;
    }
  };
  let origin = "";
  const server = http.createServer((request, response) => {
    const method = request.method || "";
    let url;
    try {
      url = new URL(request.url || "/", "http://127.0.0.1");
    } catch {
      url = null;
    }
    const pathname = url ? url.pathname : "";
    const reply = (status, body, type) => {
      record({ method, path: pathname, status });
      response.writeHead(status, { "content-type": type, "cache-control": "public, max-age=600" });
      response.end(body);
    };
    if (method !== "GET" && method !== "HEAD") {
      reply(405, JSON.stringify({ error: "isolated registry is read-only" }), "application/json");
      return;
    }
    if (pathname.startsWith("/-/tarballs/")) {
      const item = byFile.get(decodeURIComponent(pathname.slice("/-/tarballs/".length)));
      if (item) reply(200, method === "HEAD" ? undefined : item.bytes, "application/octet-stream");
      else reply(404, JSON.stringify({ error: "not staged" }), "application/json");
      return;
    }
    const name = decodeURIComponent(pathname.slice(1));
    const list = byName.get(name);
    if (list) reply(200, JSON.stringify(packument(name, list, origin)), "application/json");
    else reply(404, JSON.stringify({ error: "not staged; this registry has no upstream" }), "application/json");
  });
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen({ host: "127.0.0.1", port: 0, exclusive: true }, resolve);
  });
  const address = server.address();
  origin = `http://127.0.0.1:${address.port}`;
  let closed = null;
  const close = () => {
    if (!closed) {
      closed = new Promise((resolve) => {
        server.close(() => resolve());
        server.closeAllConnections();
      });
    }
    return closed;
  };
  return {
    origin,
    port: address.port,
    staged,
    requests,
    droppedRequestCount: () => dropped,
    close,
  };
}

// Child-process mode: the journey runs npm synchronously, so the registry
// must not share its event loop. `node registry.cjs <request-log> <tgz>...`
// prints one JSON line with the origin, appends each request to the log and
// shuts down when its stdin closes (or on SIGTERM).
async function serveFromCommandLine(argv) {
  const [requestLog, ...tarballs] = argv;
  if (!requestLog) throw new RegistryError("registry_usage", "usage: registry.cjs <request-log> <tarball>...");
  fs.writeFileSync(requestLog, "");
  const handle = await startRegistry(tarballs, { requestLog });
  process.stdout.write(`${JSON.stringify({ origin: handle.origin, port: handle.port })}\n`);
  const stop = () => handle.close().then(() => process.exit(0));
  process.stdin.on("end", stop);
  process.stdin.on("close", stop);
  process.on("SIGTERM", stop);
  process.stdin.resume();
}

if (require.main === module) {
  serveFromCommandLine(process.argv.slice(2)).catch((error) => {
    process.stderr.write(`ripr isolated registry: ${error.message}\n`);
    process.exit(2);
  });
}

module.exports = { RegistryError, readTarball, stageTarball, startRegistry };

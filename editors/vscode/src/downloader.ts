import * as cp from 'child_process';
import * as crypto from 'crypto';
import { promises as fsp } from 'fs';
import * as https from 'https';
import * as path from 'path';
import * as vscode from 'vscode';
import { RiprConfig } from './config';
import {
  distributionGeneration,
  installManagedServer,
  ManagedServerInstallation,
  ManagedServerInstallRequest,
  readManagedServerInstallation,
  safeDigestEquals,
  validateManagedServerVersion
} from './managedServerInstall';
import { RiprPlatform } from './platform';
import { SERVER_DISTRIBUTION_DESCRIPTORS, ServerDistributionDescriptor } from './serverDescriptor';

export interface ManifestAsset {
  readonly url: string;
  readonly sha256: string;
}

export interface ServerManifest {
  readonly version: string;
  readonly assets: Record<string, ManifestAsset>;
}

/**
 * How one manifest placement fetch ended (#3798 failure law): only a direct,
 * non-redirected HTTP 404 is authoritative absence; every other outcome is a
 * contradiction or transport failure and never authorizes a fallback.
 */
export type ManifestFetchOutcome =
  | { readonly kind: 'ok'; readonly bytes: Buffer }
  | { readonly kind: 'authoritative_absence'; readonly url: string }
  | {
      readonly kind: 'http_failure';
      readonly url: string;
      readonly statusCode: number;
      readonly redirected: boolean;
    }
  | { readonly kind: 'transport_failure'; readonly url: string; readonly message: string };

/** A manifest placement fetch that did not return a payload. */
export type ManifestFetchNonSuccess = Exclude<ManifestFetchOutcome, { readonly kind: 'ok' }>;

/** Maximum manifest JSON payload accepted from the download source. */
export const MANIFEST_MAX_BYTES = 1024 * 1024;

/** Maximum server archive payload accepted from the download source. */
export const ARCHIVE_MAX_BYTES = 200 * 1024 * 1024;

const REDIRECT_ALLOWED_HOSTS = new Set([
  'github.com',
  'www.github.com',
  'objects.githubusercontent.com',
  'release-assets.githubusercontent.com'
]);

export type ManifestBytesFetcher = (url: string) => Promise<ManifestFetchOutcome>;

/**
 * Placement observation, kept separate from manifest and archive content
 * identity (#3798): which exact manifest URL was selected, and whether the RC
 * placement was selected only after authoritative stable absence.
 */
export type ManifestPlacementObservation =
  | { readonly placement: 'stable'; readonly manifestUrl: string }
  | {
      readonly placement: 'rc_after_stable_absent';
      readonly stableManifestUrl: string;
      readonly rcManifestUrl: string;
    };

export interface SelectedServerManifest {
  readonly manifest: ServerManifest;
  readonly manifestUrl: string;
  readonly observation: ManifestPlacementObservation;
  /** Raw-byte SHA-256 of the manifest that was selected. */
  readonly manifestSha256: string;
}

export interface ManifestPlacementRequest {
  readonly requestedVersion: string;
  readonly generation: string;
  readonly platformTarget: string;
  readonly stableManifestUrl: string;
  readonly rcManifestUrl?: string;
  readonly admittedManifestSha256?: string;
}

export interface ManifestPlacementPlan {
  readonly generation: string;
  readonly stableManifestUrl: string;
  /** The one predeclared exact RC placement; never present for mirror routes. */
  readonly rcManifestUrl?: string;
}

export async function downloadServer(
  context: vscode.ExtensionContext,
  config: RiprConfig,
  platform: RiprPlatform,
  version: string,
  output: vscode.OutputChannel
): Promise<ManagedServerInstallation> {
  const managedVersion = validateManagedServerVersion(version);
  const origin = downloadOriginLabel(config, managedVersion);
  return vscode.window.withProgress(
    {
      location: vscode.ProgressLocation.Notification,
      title: `ripr: downloading server ${managedVersion} for ${platform.target} from ${origin}`,
      cancellable: false
    },
    (progress) => downloadServerWithProgress(context, config, platform, managedVersion, output, progress)
  );
}

async function downloadServerWithProgress(
  context: vscode.ExtensionContext,
  config: RiprConfig,
  platform: RiprPlatform,
  version: string,
  output: vscode.OutputChannel,
  progress: vscode.Progress<{ message?: string; increment?: number }>
): Promise<ManagedServerInstallation> {
  const requestedVersion = validateManagedServerVersion(version);
  const generation = distributionGeneration(requestedVersion);
  const placement = manifestPlacementPlan(config.downloadBaseUrl, requestedVersion);
  const admittedManifestSha256 = admittedDescriptor(generation)?.manifestSha256;
  const request = installRequest(context, requestedVersion, platform);
  return installManagedServer(request, {
    resolveArchive: async () => {
      progress.report({ message: 'Fetching release manifest…' });
      const selection = await resolveServerManifestPlacement(
        {
          requestedVersion,
          generation,
          platformTarget: platform.target,
          stableManifestUrl: placement.stableManifestUrl,
          rcManifestUrl: placement.rcManifestUrl,
          admittedManifestSha256
        },
        fetchManifestBytesOverHttps
      );
      output.appendLine(
        selection.observation.placement === 'stable'
          ? `Selected stable server manifest for generation ${generation} at ${selection.manifestUrl}.`
          : `Stable server manifest at ${selection.observation.stableManifestUrl} is absent; selected the exact RC placement at ${selection.manifestUrl} with the admitted manifest digest.`
      );
      const asset = selection.manifest.assets[platform.target];
      if (!asset) {
        throw new Error(`No ripr server asset is listed for ${platform.target} in manifest ${selection.manifest.version}.`);
      }

      output.appendLine(`Downloading ripr server ${version} for ${platform.target}.`);
      progress.report({ message: `Downloading ${platform.executableName}…` });
      const bytes = await fetchBuffer(selectedArchiveUrl(selection, asset));
      progress.report({ message: 'Verifying checksum…' });
      return {
        manifestVersion: selection.manifest.version,
        expectedSha256: asset.sha256,
        bytes,
        manifestUrl: selection.manifestUrl,
        manifestPlacement: selection.observation.placement,
        manifestSha256: selection.manifestSha256
      };
    },
    extractArchive: async (archivePath, destination) => {
      progress.report({ message: 'Extracting…' });
      await extractArchive(archivePath, destination, platform);
    },
    probeExecutable: (executablePath) => probeDownloadedExecutable(executablePath)
  });
}

/**
 * Resolves the manifest for one distribution generation across its placements
 * (#3798): the exact stable placement first; on authoritative stable absence,
 * the one predeclared exact RC placement admitted by the same raw-byte
 * manifest digest. Every other outcome fails closed with its original typed
 * reason — a contradiction is not absence, and transport unavailability is
 * not absence.
 */
export async function resolveServerManifestPlacement(
  request: ManifestPlacementRequest,
  fetchManifestBytes: ManifestBytesFetcher = fetchManifestBytesOverHttps
): Promise<SelectedServerManifest> {
  const stable = await fetchManifestBytes(request.stableManifestUrl);
  if (stable.kind === 'ok') {
    const manifest = parseAdmittedManifest(stable.bytes, request.admittedManifestSha256, request.stableManifestUrl);
    assertPlacementIdentity(manifest, request, request.stableManifestUrl);
    return {
      manifest,
      manifestUrl: request.stableManifestUrl,
      observation: { placement: 'stable', manifestUrl: request.stableManifestUrl },
      manifestSha256: sha256Hex(stable.bytes)
    };
  }
  if (stable.kind !== 'authoritative_absence') {
    throw placementFetchError(stable);
  }
  if (!request.rcManifestUrl || !request.admittedManifestSha256) {
    throw new Error(
      `Stable server manifest is absent (${request.stableManifestUrl} returned 404) and no admitted RC fallback placement is available for ${request.requestedVersion}.`
    );
  }
  const rc = await fetchManifestBytes(request.rcManifestUrl);
  if (rc.kind !== 'ok') {
    throw placementFetchError(rc);
  }
  const rcManifest = parseAdmittedManifest(rc.bytes, request.admittedManifestSha256, request.rcManifestUrl);
  assertPlacementIdentity(rcManifest, request, request.rcManifestUrl);
  return {
    manifest: rcManifest,
    manifestUrl: request.rcManifestUrl,
    observation: {
      placement: 'rc_after_stable_absent',
      stableManifestUrl: request.stableManifestUrl,
      rcManifestUrl: request.rcManifestUrl
    },
    manifestSha256: sha256Hex(rc.bytes)
  };
}

/**
 * The manifest placements for a requested server version (#3798). The
 * manifest file is keyed by the distribution generation; on the default
 * GitHub release route a prerelease request carries exactly one predeclared
 * RC fallback row — its own release tag serving the same generation-keyed
 * manifest. Configured mirror routes serve a single placement directory and
 * have no accepted RC route.
 */
export function manifestPlacementPlan(downloadBaseUrl: string, requestedVersion: string): ManifestPlacementPlan {
  const generation = distributionGeneration(requestedVersion);
  const manifestFile = `ripr-server-manifest-v${generation}.json`;
  const base = downloadBaseUrl.trim();
  if (base.length > 0) {
    return { generation, stableManifestUrl: `${base.replace(/\/+$/, '')}/${manifestFile}` };
  }
  const stableManifestUrl = `https://github.com/EffortlessMetrics/ripr/releases/download/v${generation}/${manifestFile}`;
  if (requestedVersion === generation) {
    return { generation, stableManifestUrl };
  }
  return {
    generation,
    stableManifestUrl,
    rcManifestUrl: `https://github.com/EffortlessMetrics/ripr/releases/download/v${requestedVersion}/${manifestFile}`
  };
}

/**
 * The archive URL on the placement that served the manifest: RC placements
 * carry the same generation-keyed archive names, so the archive is fetched
 * beside the manifest that admitted it — identical archive identity, observed
 * placement (#3798).
 */
export function placementArchiveUrl(manifestUrl: string, assetUrl: string): string {
  const fileName = assetUrl.substring(assetUrl.lastIndexOf('/') + 1);
  if (fileName.length === 0) {
    throw new Error(`Server manifest asset URL ${assetUrl} does not name an archive file.`);
  }
  const baseDirectory = manifestUrl.substring(0, manifestUrl.lastIndexOf('/') + 1);
  return `${baseDirectory}${fileName}`;
}

export function cachedServerInstallation(
  context: vscode.ExtensionContext,
  version: string,
  platform: RiprPlatform
): Promise<ManagedServerInstallation | undefined> {
  return readManagedServerInstallation(installRequest(context, version, platform));
}

function admittedDescriptor(generation: string): ServerDistributionDescriptor | undefined {
  return SERVER_DISTRIBUTION_DESCRIPTORS.find(
    (descriptor) => descriptor.generation === generation && /^[0-9a-f]{64}$/i.test(descriptor.manifestSha256)
  );
}

function selectedArchiveUrl(selection: SelectedServerManifest, asset: ManifestAsset): string {
  if (selection.observation.placement === 'stable') {
    return asset.url;
  }
  return placementArchiveUrl(selection.manifestUrl, asset.url);
}

function installRequest(
  context: vscode.ExtensionContext,
  version: string,
  platform: RiprPlatform
): ManagedServerInstallRequest {
  const admittedManifestSha256 = admittedDescriptor(distributionGeneration(version))?.manifestSha256;
  return {
    serversRoot: path.join(context.globalStorageUri.fsPath, 'servers'),
    version,
    platformTarget: platform.target,
    executableName: platform.executableName,
    archiveExtension: platform.archiveExtension,
    ...(admittedManifestSha256 !== undefined ? { admittedManifestSha256 } : {})
  };
}

function downloadOriginLabel(config: RiprConfig, version: string): string {
  try {
    return new URL(manifestPlacementPlan(config.downloadBaseUrl, version).stableManifestUrl).host;
  } catch {
    return 'the configured download mirror';
  }
}

function sha256Hex(bytes: Buffer): string {
  return crypto.createHash('sha256').update(bytes).digest('hex');
}

function parseAdmittedManifest(
  bytes: Buffer,
  admittedManifestSha256: string | undefined,
  url: string
): ServerManifest {
  if (admittedManifestSha256 !== undefined) {
    const observed = crypto.createHash('sha256').update(bytes).digest('hex');
    if (!safeDigestEquals(observed, admittedManifestSha256)) {
      throw new Error(
        `Server manifest at ${url} does not match the admitted manifest SHA-256 for its distribution generation.`
      );
    }
  }
  const parsed: unknown = JSON.parse(bytes.toString('utf8'));
  if (!parsed || typeof parsed !== 'object') {
    throw new Error('Server manifest is not an object.');
  }
  const manifest = parsed as Record<string, unknown>;
  if (typeof manifest.version !== 'string' || !manifest.assets || typeof manifest.assets !== 'object') {
    throw new Error('Server manifest is missing a string version or asset map.');
  }
  for (const [target, value] of Object.entries(manifest.assets as Record<string, unknown>)) {
    if (!value || typeof value !== 'object') {
      throw new Error(`Server manifest asset ${target} is not an object.`);
    }
    const asset = value as Record<string, unknown>;
    if (typeof asset.url !== 'string' || typeof asset.sha256 !== 'string') {
      throw new Error(`Server manifest asset ${target} is missing its URL or SHA-256 digest.`);
    }
  }
  return parsed as ServerManifest;
}

function assertPlacementIdentity(manifest: ServerManifest, request: ManifestPlacementRequest, url: string): void {
  if (manifest.version !== request.generation) {
    throw new Error(
      `Server manifest version ${manifest.version} at ${url} does not match distribution generation ${request.generation}.`
    );
  }
  if (!manifest.assets[request.platformTarget]) {
    throw new Error(`No ripr server asset is listed for ${request.platformTarget} in the manifest at ${url}.`);
  }
}

function placementFetchError(outcome: ManifestFetchNonSuccess): Error {
  if (outcome.kind === 'authoritative_absence') {
    return new Error(`GET ${outcome.url} failed with HTTP 404.`);
  }
  if (outcome.kind === 'http_failure') {
    const redirected = outcome.redirected ? ' after redirect' : '';
    return new Error(`GET ${outcome.url} failed with HTTP ${outcome.statusCode}${redirected}.`);
  }
  return new Error(outcome.message);
}

export function isAllowedHttpsRedirect(fromUrl: string, toUrl: string): boolean {
  let from: URL;
  let to: URL;
  try {
    from = new URL(fromUrl);
    to = new URL(toUrl);
  } catch {
    return false;
  }
  if (to.protocol !== 'https:') {
    return false;
  }
  if (to.host === from.host) {
    return true;
  }
  return REDIRECT_ALLOWED_HOSTS.has(to.host);
}

export function fetchManifestBytesOverHttps(url: string, maxBytes = MANIFEST_MAX_BYTES): Promise<ManifestFetchOutcome> {
  return new Promise((resolve) => {
    let settled = false;
    const finish = (outcome: ManifestFetchOutcome): void => {
      if (settled) {
        return;
      }
      settled = true;
      resolve(outcome);
    };
    const attempt = (target: string, redirects: number): void => {
      // Request creation can throw synchronously: a redirect `Location` may
      // resolve to a non-`https:` URL, and the https request call rejects
      // such a
      // protocol (`ERR_INVALID_PROTOCOL`) before any callback exists. On a
      // redirect hop that throw escapes the promise context as an uncaught
      // exception while this promise never settles, so the refusal and the
      // whole request setup must resolve the typed transport failure instead
      // (#3798: fail closed, never crash, never hang).
      let request: import('http').ClientRequest;
      try {
        if (new URL(target).protocol !== 'https:') {
          finish({
            kind: 'transport_failure',
            url: target,
            message: `Refusing non-HTTPS manifest URL ${target}.`
          });
          return;
        }
        request = https.get(target, (response) => {
          const statusCode = response.statusCode ?? 0;
          const location = response.headers.location;
          if (statusCode >= 300 && statusCode < 400 && location) {
            response.resume();
            if (redirects >= 5) {
              finish({
                kind: 'transport_failure',
                url: target,
                message: `Too many redirects while fetching ${target}.`
              });
              return;
            }
            let redirectedUrl: string;
            try {
              redirectedUrl = new URL(location, target).toString();
            } catch (error) {
              finish({
                kind: 'transport_failure',
                url: target,
                message: `Invalid redirect from ${target}: ${error instanceof Error ? error.message : String(error)}`
              });
              return;
            }
            if (!isAllowedHttpsRedirect(target, redirectedUrl)) {
              finish({
                kind: 'transport_failure',
                url: target,
                message: `Refusing redirect from ${target} to disallowed host ${new URL(redirectedUrl).host}.`
              });
              return;
            }
            attempt(redirectedUrl, redirects + 1);
            return;
          }
          if (statusCode === 404 && redirects === 0) {
            response.resume();
            finish({ kind: 'authoritative_absence', url: target });
            return;
          }
          if (statusCode < 200 || statusCode >= 300) {
            response.resume();
            finish({ kind: 'http_failure', url: target, statusCode, redirected: redirects > 0 });
            return;
          }

          const contentLengthHeader = response.headers['content-length'];
          if (contentLengthHeader !== undefined) {
            const declaredLength = Number(contentLengthHeader);
            if (Number.isFinite(declaredLength) && declaredLength > maxBytes) {
              request.destroy();
              finish({
                kind: 'transport_failure',
                url: target,
                message: `Response Content-Length ${declaredLength} exceeds the ${maxBytes} byte limit while fetching ${target}.`
              });
              return;
            }
          }

          const chunks: Buffer[] = [];
          let receivedBytes = 0;
          response.on('data', (chunk: Buffer) => {
            receivedBytes += chunk.length;
            if (receivedBytes > maxBytes) {
              request.destroy();
              finish({
                kind: 'transport_failure',
                url: target,
                message: `Response body exceeds the ${maxBytes} byte limit while fetching ${target}.`
              });
              return;
            }
            chunks.push(chunk);
          });
          response.on('end', () => finish({ kind: 'ok', bytes: Buffer.concat(chunks) }));
          // A connection that dies mid-body closes the response without
          // `end`; without this guard the promise would pend forever. The
          // resolve is idempotent, so a complete response keeps its `ok`.
          response.on('close', () => {
            if (!response.complete) {
              finish({
                kind: 'transport_failure',
                url: target,
                message: `Manifest response from ${target} closed before completion.`
              });
            }
          });
          response.on('error', (error) => {
            finish({
              kind: 'transport_failure',
              url: target,
              message: error instanceof Error ? error.message : String(error)
            });
          });
        });
        request.on('error', (error) => {
          // A raw socket error ("getaddrinfo ENOTFOUND host") does not say
          // what was being fetched; the fetch timeout already names it.
          const message = error.message.includes(target)
            ? error.message
            : `Could not fetch ${target}: ${error.message}`;
          finish({ kind: 'transport_failure', url: target, message });
        });
        request.setTimeout(30_000, () => {
          request.destroy(new Error(`Timed out while fetching ${target}.`));
        });
      } catch (error) {
        finish({
          kind: 'transport_failure',
          url: target,
          message: error instanceof Error ? error.message : String(error)
        });
      }
    };
    attempt(url, 0);
  });
}

async function fetchBuffer(url: string): Promise<Buffer> {
  const outcome = await fetchManifestBytesOverHttps(url, ARCHIVE_MAX_BYTES);
  if (outcome.kind === 'ok') {
    return outcome.bytes;
  }
  throw placementFetchError(outcome);
}

async function extractArchive(archivePath: string, destination: string, platform: RiprPlatform): Promise<void> {
  if (platform.archiveExtension === 'zip') {
    await inspectZipArchive(archivePath);
    await runProcess('powershell.exe', [
      '-NoProfile',
      '-ExecutionPolicy',
      'Bypass',
      '-Command',
      `Expand-Archive -LiteralPath ${quotePowerShell(archivePath)} -DestinationPath ${quotePowerShell(destination)} -Force`
    ]);
    return;
  }
  await inspectTarArchive(archivePath);
  await runProcess('tar', ['-xzf', archivePath, '-C', destination]);
}

/** Maximum member count accepted from a server archive before extraction. */
export const ARCHIVE_MAX_MEMBERS = 1000;

/**
 * Lists a tar archive and refuses unsafe members before anything is written
 * (#13640): the post-extraction walk cannot undo writes made through a symlink
 * or `..` member, so those members must never reach `tar -x`.
 */
export async function inspectTarArchive(archivePath: string): Promise<void> {
  const names = await runProcess('tar', ['-tzf', archivePath]);
  const verbose = await runProcess('tar', ['-tvzf', archivePath]);
  assertSafeTarListing(names.split(/\r?\n/), verbose.split(/\r?\n/));
}

/**
 * Pure policy over `tar -tzf` names and `tar -tvzf` lines: rejects absolute or
 * drive-qualified paths, `..` segments, NUL/backslash tricks, symlink and
 * hard-link members, any non-file/directory type, and oversized member counts.
 */
export function assertSafeTarListing(nameLines: readonly string[], verboseLines: readonly string[]): void {
  const names = nameLines.filter((line) => line.length > 0);
  if (names.length > ARCHIVE_MAX_MEMBERS) {
    throw new Error(`Server archive lists ${names.length} members, over the ${ARCHIVE_MAX_MEMBERS} member limit.`);
  }
  for (const name of names) {
    const segments = name.split(/[\\/]/);
    if (/^[\\/]/.test(name) || /^[A-Za-z]:/.test(name) || name.includes('\0') || segments.includes('..')) {
      throw new Error(`Refusing server archive member with unsafe path ${JSON.stringify(name)}.`);
    }
  }
  for (const line of verboseLines) {
    if (line.length === 0) {
      continue;
    }
    // GNU tar marks links with a leading 'l'/'h'; bsdtar prints hard links as
    // regular rows containing "link to". Anything but '-' or 'd' is refused.
    const kind = line[0];
    if (kind !== '-' && kind !== 'd') {
      throw new Error(`Refusing server archive member of unsupported type: ${JSON.stringify(line)}.`);
    }
    if (/ link to /.test(line)) {
      throw new Error(`Refusing server archive hard link member: ${JSON.stringify(line)}.`);
    }
  }
}

/** One central-directory entry as read from a zip archive. */
export interface ZipCentralDirectoryEntry {
  readonly name: string;
  readonly externalAttributes: number;
}

const ZIP_EOCD_SIGNATURE = 0x06054b50;
const ZIP_CENTRAL_SIGNATURE = 0x02014b50;
/** General-purpose flag bit 11: the member name is UTF-8, not cp437. */
const ZIP_GP_FLAG_UTF8 = 0x0800;
/** DOS attribute mask inside `externalAttributes`: FILE_ATTRIBUTE_REPARSE_POINT. */
const ZIP_DOS_REPARSE_POINT = 0x0400;
/** Unix mode mask inside the high 16 bits of `externalAttributes`. */
const ZIP_UNIX_IFMT = 0xf000;
const ZIP_UNIX_IFREG = 0x8000;
const ZIP_UNIX_IFDIR = 0x4000;
const ZIP_MAX_COMMENT_BYTES = 0xffff;

/**
 * Reads the central directory of a zip archive without extracting anything
 * (#13904): the listing must be validated before `Expand-Archive` writes a
 * single member, the same pre-extraction contract `inspectTarArchive` keeps
 * for the tar path (#13640).
 */
export function listZipCentralDirectoryEntries(archive: Buffer): ZipCentralDirectoryEntry[] {
  // The end-of-central-directory record can sit anywhere in the last
  // 22 + 65,535 bytes (the whole trailing comment range).
  const searchFloor = Math.max(0, archive.length - (22 + ZIP_MAX_COMMENT_BYTES));
  let eocd = -1;
  for (let cursor = archive.length - 22; cursor >= searchFloor; cursor -= 1) {
    if (archive.readUInt32LE(cursor) === ZIP_EOCD_SIGNATURE) {
      eocd = cursor;
      break;
    }
  }
  if (eocd < 0) {
    throw new Error('Server archive is not a readable zip: end-of-central-directory record not found.');
  }
  if (archive.readUInt16LE(eocd + 4) !== 0 || archive.readUInt16LE(eocd + 6) !== 0) {
    throw new Error('Refusing multi-disk server archive.');
  }
  const recordedCount = archive.readUInt16LE(eocd + 10);
  const centralSize = archive.readUInt32LE(eocd + 12);
  const centralOffset = archive.readUInt32LE(eocd + 16);
  if (centralOffset + centralSize > archive.length) {
    throw new Error('Refusing server archive: central directory extends past the end of the file.');
  }
  const entries: ZipCentralDirectoryEntry[] = [];
  let cursor = centralOffset;
  const centralEnd = centralOffset + centralSize;
  while (cursor < centralEnd) {
    if (cursor + 46 > centralEnd || archive.readUInt32LE(cursor) !== ZIP_CENTRAL_SIGNATURE) {
      throw new Error('Refusing server archive: malformed central directory entry.');
    }
    const flags = archive.readUInt16LE(cursor + 8);
    const nameLength = archive.readUInt16LE(cursor + 28);
    const extraLength = archive.readUInt16LE(cursor + 30);
    const commentLength = archive.readUInt16LE(cursor + 32);
    if (46 + nameLength + extraLength + commentLength > centralEnd - cursor) {
      throw new Error('Refusing server archive: central directory entry extends past the directory end.');
    }
    const externalAttributes = archive.readUInt32LE(cursor + 38);
    const nameBytes = archive.subarray(cursor + 46, cursor + 46 + nameLength);
    // Expand-Archive reads the same flag; a non-UTF-8 name is decoded as
    // single bytes here and refused below if it spells anything unsafe.
    const name = (flags & ZIP_GP_FLAG_UTF8) !== 0 ? nameBytes.toString('utf8') : nameBytes.toString('latin1');
    entries.push({ name, externalAttributes });
    cursor += 46 + nameLength + extraLength + commentLength;
  }
  if (entries.length !== recordedCount) {
    throw new Error(`Refusing server archive: central directory holds ${entries.length} entries, expected ${recordedCount}.`);
  }
  return entries;
}

/**
 * Pure policy over a zip central-directory listing, mirroring
 * `assertSafeTarListing`: rejects traversal, absolute, drive-qualified or NUL
 * names, symlink members (Unix mode in the external attributes), NTFS reparse
 * points (DOS attributes), any other non-file/directory member type, and
 * oversized member counts.
 */
export function assertSafeZipListing(entries: readonly ZipCentralDirectoryEntry[]): void {
  if (entries.length > ARCHIVE_MAX_MEMBERS) {
    throw new Error(`Server archive lists ${entries.length} members, over the ${ARCHIVE_MAX_MEMBERS} member limit.`);
  }
  for (const entry of entries) {
    const segments = entry.name.split(/[\\/]/);
    if (/^[\\/]/.test(entry.name) || /^[A-Za-z]:/.test(entry.name) || entry.name.includes('\0') || segments.includes('..')) {
      throw new Error(`Refusing server archive member with unsafe path ${JSON.stringify(entry.name)}.`);
    }
    if ((entry.externalAttributes & ZIP_DOS_REPARSE_POINT) !== 0) {
      throw new Error(`Refusing server archive reparse-point member: ${JSON.stringify(entry.name)}.`);
    }
    const unixMode = (entry.externalAttributes >>> 16) & 0xffff;
    if (unixMode !== 0) {
      // Windows tools (Python zipfile.writestr and friends) store permission
      // bits without a file type, so only a *present* type is classified;
      // anything typed that is not a regular file or directory is refused,
      // which catches symlinks (0o120000) and device/fifo members.
      const fileType = unixMode & ZIP_UNIX_IFMT;
      if (fileType !== 0 && fileType !== ZIP_UNIX_IFREG && fileType !== ZIP_UNIX_IFDIR) {
        throw new Error(`Refusing server archive member of unsupported type: ${JSON.stringify(entry.name)}.`);
      }
    }
  }
}

/**
 * Lists and validates a zip archive's members before extraction (#13904), so
 * symlink and reparse-point entries never reach `Expand-Archive`.
 */
export async function inspectZipArchive(archivePath: string): Promise<void> {
  assertSafeZipListing(listZipCentralDirectoryEntries(await fsp.readFile(archivePath)));
}

function runProcess(command: string, args: string[]): Promise<string> {
  return new Promise((resolve, reject) => {
    cp.execFile(command, args, { maxBuffer: 16 * 1024 * 1024 }, (error, stdout, stderr) => {
      if (error) {
        reject(new Error(stderr.trim() || error.message));
      } else {
        resolve(stdout);
      }
    });
  });
}

function quotePowerShell(value: string): string {
  return `'${value.replace(/'/g, "''")}'`;
}

function probeDownloadedExecutable(executablePath: string): Promise<string> {
  return new Promise((resolve, reject) => {
    cp.execFile(executablePath, ['--version'], { timeout: 5000 }, (error, stdout, stderr) => {
      if (error) {
        reject(new Error(`Downloaded server failed its version probe: ${stderr.trim() || error.message}`));
        return;
      }
      const version = firstNonemptyLine(stdout, stderr);
      if (!version) {
        reject(new Error('Downloaded server version probe produced no version text.'));
        return;
      }
      resolve(version);
    });
  });
}

function firstNonemptyLine(stdout: string, stderr: string): string | undefined {
  return (stdout || stderr)
    .split(/\r?\n/)
    .map((line) => line.trim())
    .find((line) => line.length > 0);
}

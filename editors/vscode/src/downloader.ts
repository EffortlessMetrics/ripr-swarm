import * as cp from 'child_process';
import * as crypto from 'crypto';
import * as https from 'https';
import * as path from 'path';
import * as vscode from 'vscode';
import { extractAdmittedArchive } from './archiveInventory';
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
  output: vscode.OutputChannel,
  distribution?: ResolvedDistributionRequest
): Promise<ManagedServerInstallation> {
  const managedVersion = validateManagedServerVersion(version);
  const origin = downloadOriginLabel(config, managedVersion);
  return vscode.window.withProgress(
    {
      location: vscode.ProgressLocation.Notification,
      title: `ripr: downloading server ${managedVersion} for ${platform.target} from ${origin}`,
      cancellable: false
    },
    (progress) =>
      downloadServerWithProgress(context, config, platform, managedVersion, output, progress, distribution)
  );
}

async function downloadServerWithProgress(
  context: vscode.ExtensionContext,
  config: RiprConfig,
  platform: RiprPlatform,
  version: string,
  output: vscode.OutputChannel,
  progress: vscode.Progress<{ message?: string; increment?: number }>,
  distribution?: ResolvedDistributionRequest
): Promise<ManagedServerInstallation> {
  const requestedVersion = validateManagedServerVersion(version);
  const generation = distributionGeneration(requestedVersion);
  const placement = manifestPlacementPlan(config.downloadBaseUrl, requestedVersion);
  const admittedManifestSha256 = admittedDescriptor(generation)?.manifestSha256;
  const request = installRequest(context, requestedVersion, platform);
  return installManagedServer(request, {
    resolveArchive: async () => {
      // Descriptor-bound downloads never fetch or parse unadmitted bytes:
      // branch before any manifest fetch so a replaced manifest cannot even
      // be retrieved, let alone select its own asset host.
      if (distribution?.manifestSha256 !== undefined) {
        return downloadAdmittedAsset(config, distribution, platform, version, output, progress);
      }

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
      // Inventory authority (#1641): checksum-verified bytes are decoded by
      // the owned parser, policed, and budgeted before any file is created.
      // No system extractor ever touches archive bytes on this path.
      await extractAdmittedArchive(
        await fs.promises.readFile(archivePath),
        platform.archiveExtension,
        destination,
        platform.executableName
      );
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
  platform: RiprPlatform,
  distribution?: ResolvedDistributionRequest
): Promise<ManagedServerInstallation | undefined> {
  return readManagedServerInstallation(installRequest(context, version, platform, distribution));
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
  platform: RiprPlatform,
  distribution?: ResolvedDistributionRequest
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

/**
 * Descriptor-bound asset download. The manifest digest gates the raw bytes
 * before any field is read; the asset URL composes from the accepted
 * placement plus the admitted bare subject, so a replaced manifest cannot
 * select its own host. The returned admission stamp flows into the install
 * receipt, binding the cache entry to the exact descriptor/manifest tuple.
 */
async function downloadAdmittedAsset(
  config: RiprConfig,
  distribution: ResolvedDistributionRequest,
  platform: RiprPlatform,
  version: string,
  output: vscode.OutputChannel,
  progress: vscode.Progress<{ message?: string; increment?: number }>
): Promise<ResolvedArchive> {
  const expectedDigest = distribution.manifestSha256 as string;
  const admitted = await fetchAdmittedManifestForDistribution(
    config.downloadBaseUrl,
    distribution,
    version,
    expectedDigest
  );
  if (admitted.manifest.productVersion !== version) {
    throw new Error(
      `Admitted manifest product version ${admitted.manifest.productVersion} does not match requested version ${version}.`
    );
  }
  const asset = admitted.manifest.assets[platform.target];
  if (!asset) {
    throw new Error(`No ripr server asset is listed for ${platform.target} in the admitted manifest.`);
  }
  const placementBase = admitted.manifestUrl.slice(0, admitted.manifestUrl.lastIndexOf('/'));
  const assetUrl = assetUrlForSubject(placementBase, asset.subject);
  output.appendLine(`Downloading ripr server ${version} for ${platform.target} from admitted placement.`);
  progress.report({ message: `Downloading ${platform.executableName}…` });
  const { body: bytes } = await fetchBuffer(assetUrl, asset.archiveSize, fetchPolicyFor(assetUrl));
  progress.report({ message: 'Verifying checksum…' });
  return {
    manifestVersion: admitted.manifest.productVersion,
    expectedSha256: asset.sha256,
    bytes,
    admittedManifestSha256: expectedDigest,
    selectedManifestUrl: admitted.manifestUrl,
    manifestSelection: admitted.manifestSelection,
    preferredManifestObservation: admitted.preferredManifestObservation,
    fallbackManifestObservation: admitted.fallbackManifestObservation
  };
}

export type FetchAdmittedManifest = (
  url: string,
  expectedDigest: string
) => Promise<AdmittedServerManifest>;

export interface AdmittedManifestSelection {
  readonly manifest: AdmittedServerManifest;
  readonly manifestUrl: string;
  readonly manifestSelection: 'preferred_exact' | 'fallback_exact_after_preferred_absent';
  readonly preferredManifestObservation: 'accepted' | 'direct_not_found';
  readonly fallbackManifestObservation: 'not_requested' | 'accepted';
}

/**
 * Fetches the descriptor-bound manifest from the preferred exact placement.
 * Only an authoritative direct 404 may select the one predeclared fallback.
 * Both placements are admitted against the same embedded manifest digest, so
 * fallback changes the observed location without changing content authority.
 */
export async function fetchAdmittedManifestForDistribution(
  baseUrl: string,
  distribution: ResolvedDistributionRequest,
  version: string,
  expectedDigest: string,
  fetchImpl: FetchAdmittedManifest = fetchAdmittedManifest
): Promise<AdmittedManifestSelection> {
  const [preferred, ...fallbacks] = manifestCandidatesForDistribution(baseUrl, distribution, version);
  try {
    return {
      manifest: await fetchImpl(preferred, expectedDigest),
      manifestUrl: preferred,
      manifestSelection: 'preferred_exact',
      preferredManifestObservation: 'accepted',
      fallbackManifestObservation: 'not_requested'
    };
  } catch (error) {
    const fallback = fallbacks[0];
    if (fallback === undefined || !isDirectManifestNotFound(error)) {
      throw error;
    }
    return {
      manifest: await fetchImpl(fallback, expectedDigest),
      manifestUrl: fallback,
      manifestSelection: 'fallback_exact_after_preferred_absent',
      preferredManifestObservation: 'direct_not_found',
      fallbackManifestObservation: 'accepted'
    };
  }
}

export async function fetchAdmittedManifest(url: string, expectedDigest: string): Promise<AdmittedServerManifest> {
  const { body } = await fetchBuffer(url, MAX_MANIFEST_BYTES, fetchPolicyFor(url));
  return admitManifestBytes(body, expectedDigest);
}

export function fetchPolicyFor(url: string): RedirectPolicy {
  let host = '';
  try {
    host = new URL(url).hostname;
  } catch {
    throw new Error(`Release URL ${JSON.stringify(url)} is not a valid URL.`);
  }
  return { initialHost: host, admittedHosts: RELEASE_ASSET_HOSTS };
}

/**
 * Ordered release-manifest URLs for one download: the preferred placement
 * first, then the bounded predeclared fallbacks. A configured mirror is an
 * explicit transport choice and serves the preferred placement only — it
 * never falls through to RC. Without a descriptor the legacy
 * version-pinned URL is the only candidate.
 */
export function manifestCandidatesForDistribution(
  baseUrl: string,
  distribution: ResolvedDistributionRequest | undefined,
  version: string
): string[] {
  if (!distribution) {
    return [manifestUrl(baseUrl, version)];
  }
  if (baseUrl.trim().length > 0) {
    return [distributionManifestUrl(baseUrl, distribution, distribution.preferredPlacement)];
  }
  return distributionPlacements(distribution).map((placement) =>
    distributionManifestUrl('', distribution, placement)
  );
}

/**
 * Failure of one manifest fetch that records whether the candidate is
 * simply unpublished. Only a direct (non-redirected) initial HTTP 404
 * means "this placement has no manifest"; every other failure —
 * transport errors, timeouts, HTTP 5xx, redirect-chain failures, malformed
 * bodies — propagates instead of selecting a different placement.
 */
export class ManifestFetchError extends Error {
  readonly statusCode?: number;
  readonly redirected: boolean;

  constructor(message: string, options: { statusCode?: number; redirected: boolean }) {
    super(message);
    this.name = 'ManifestFetchError';
    this.statusCode = options.statusCode;
    this.redirected = options.redirected;
  }
}

/** True when a manifest fetch failed because the candidate URL is unpublished. */
export function isDirectManifestNotFound(error: unknown): boolean {
  return error instanceof ManifestFetchError && error.statusCode === 404 && !error.redirected;
}

export type FetchManifest = (url: string) => Promise<ServerManifest>;

export async function fetchManifestForDistribution(
  baseUrl: string,
  distribution: ResolvedDistributionRequest | undefined,
  version: string,
  fetchImpl: FetchManifest = fetchManifest
): Promise<ServerManifest> {
  // The candidate list holds the preferred placement first and at most one
  // bounded predeclared fallback. Only the preferred placement may fall
  // through to that fallback, and only when its own initial response is a
  // direct 404. Every other failure propagates with its original error, and
  // a fetched manifest is validated by the caller, which fails fast: a
  // contradictory manifest must not be masked by a fallback.
  const [preferred, ...fallbacks] = manifestCandidatesForDistribution(baseUrl, distribution, version);
  try {
    return await fetchImpl(preferred);
  } catch (error) {
    const fallback = fallbacks[0];
    if (fallback === undefined || !isDirectManifestNotFound(error)) {
      throw error;
    }
    return fetchImpl(fallback);
  }
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
          finish({ kind: 'transport_failure', url: target, message: error.message });
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

function extractArchive(archivePath: string, destination: string, platform: RiprPlatform): Promise<void> {
  if (platform.archiveExtension === 'zip') {
    return runProcess('powershell.exe', [
      '-NoProfile',
      '-ExecutionPolicy',
      'Bypass',
      '-Command',
      `Expand-Archive -LiteralPath ${quotePowerShell(archivePath)} -DestinationPath ${quotePowerShell(destination)} -Force`
    ]);
  }
  return runProcess('tar', ['-xzf', archivePath, '-C', destination]);
}

function runProcess(command: string, args: string[]): Promise<void> {
  return new Promise((resolve, reject) => {
    cp.execFile(command, args, (error, _stdout, stderr) => {
      if (error) {
        reject(new Error(stderr.trim() || error.message));
      } else {
        resolve();
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

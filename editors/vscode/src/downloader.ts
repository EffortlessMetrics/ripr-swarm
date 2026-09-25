import * as cp from 'child_process';
import * as crypto from 'crypto';
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
        manifestPlacement: selection.observation.placement
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
      observation: { placement: 'stable', manifestUrl: request.stableManifestUrl }
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
    }
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
  return {
    serversRoot: path.join(context.globalStorageUri.fsPath, 'servers'),
    version,
    platformTarget: platform.target,
    executableName: platform.executableName,
    archiveExtension: platform.archiveExtension
  };
}

function downloadOriginLabel(config: RiprConfig, version: string): string {
  try {
    return new URL(manifestPlacementPlan(config.downloadBaseUrl, version).stableManifestUrl).host;
  } catch {
    return 'the configured download mirror';
  }
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

function fetchManifestBytesOverHttps(url: string): Promise<ManifestFetchOutcome> {
  return new Promise((resolve) => {
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
          resolve({
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
              resolve({
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
              resolve({
                kind: 'transport_failure',
                url: target,
                message: `Invalid redirect from ${target}: ${error instanceof Error ? error.message : String(error)}`
              });
              return;
            }
            attempt(redirectedUrl, redirects + 1);
            return;
          }
          if (statusCode === 404 && redirects === 0) {
            response.resume();
            resolve({ kind: 'authoritative_absence', url: target });
            return;
          }
          if (statusCode < 200 || statusCode >= 300) {
            response.resume();
            resolve({ kind: 'http_failure', url: target, statusCode, redirected: redirects > 0 });
            return;
          }

          const chunks: Buffer[] = [];
          response.on('data', (chunk: Buffer) => chunks.push(chunk));
          response.on('end', () => resolve({ kind: 'ok', bytes: Buffer.concat(chunks) }));
        });
        request.on('error', (error) => {
          resolve({ kind: 'transport_failure', url: target, message: error.message });
        });
        request.setTimeout(30_000, () => {
          request.destroy(new Error(`Timed out while fetching ${target}.`));
        });
      } catch (error) {
        resolve({
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
  const outcome = await fetchManifestBytesOverHttps(url);
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

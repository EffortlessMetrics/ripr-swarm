import * as cp from 'child_process';
import * as fs from 'fs';
import * as path from 'path';
import * as vscode from 'vscode';
import { RiprConfig } from './config';
import { cachedServerInstallation, downloadServer } from './downloader';
import { ManagedServerInstallation } from './managedServerInstall';
import { currentRiprPlatform, RiprPlatform } from './platform';

const START_TIMEOUT_MS = 5000;

export type ServerSource = 'configured' | 'bundled' | 'managed_cache' | 'managed_download' | 'path';

export interface ResolvedServer {
  readonly command: string;
  readonly source: ServerSource;
  readonly detail: string;
  readonly binaryVersion?: string;
  readonly protocolVersion?: string;
  readonly assetDigest?: string;
  readonly installationState: 'unmanaged' | 'bundled' | 'complete';
  readonly compatibilityResult: 'not_established';
  /**
   * True when this server must be spawned through the shell (#2079): a
   * Windows `.cmd`/`.bat` PATH shim resolves via the shell probe, and the
   * client spawn must use the same launch semantics or startup fails the
   * same way the probe used to.
   */
  readonly needsShell?: boolean;
}

export interface ResolveFailure {
  readonly message: string;
  readonly detail: string;
}

export async function resolveServer(
  context: vscode.ExtensionContext,
  config: RiprConfig,
  output: vscode.OutputChannel
): Promise<ResolvedServer | ResolveFailure> {
  const configuredPath = config.serverPath.trim();
  if (configuredPath.length > 0) {
    return probeCandidate(configuredPath, 'configured', `configured ripr.server.path ${configuredPath}`, false, 'unmanaged');
  }

  const platform = currentRiprPlatform();
  const version = requestedServerVersion(context, config);
  let downloadFailure: string | undefined;

  if (platform) {
    const bundled = bundledServerPath(context, platform);
    const bundledResult = await probeExistingCandidate(bundled, 'bundled', `bundled server for ${platform.target}`);
    if (isResolved(bundledResult)) {
      return bundledResult;
    }

    const cached = await cachedServerInstallation(context, version, platform);
    if (cached) {
      const cachedResult = await probeCandidate(
        cached.executablePath,
        'managed_cache',
        `completed cached server ${version} for ${platform.target}`,
        false,
        'complete'
      );
      if (isResolved(cachedResult)) {
        return withManagedIdentity(cachedResult, cached);
      }
    }

    if (config.autoDownload) {
      try {
        const downloaded = await downloadServer(context, config, platform, version, output);
        const downloadedResult = await probeCandidate(
          downloaded.executablePath,
          'managed_download',
          `atomically installed server ${version} for ${platform.target}`,
          false,
          'complete'
        );
        if (isResolved(downloadedResult)) {
          return withManagedIdentity(downloadedResult, downloaded);
        }
        downloadFailure = downloadedResult.detail;
      } catch (error) {
        downloadFailure = error instanceof Error ? error.message : String(error);
        output.appendLine(`ripr server download failed: ${downloadFailure}`);
      }
    }
  } else {
    downloadFailure = `No prebuilt ripr server target is known for ${process.platform}/${process.arch}.`;
  }

  // On Windows, spawning with shell: false does no PATHEXT resolution, so
  // ripr.bat/ripr.cmd shims (Scoop, Chocolatey, manual PATH) fail to start
  // even though `ripr` works in a terminal (#2079). The command is the
  // constant string 'ripr --version' — no user input reaches the shell.
  const probeWithShell = process.platform === 'win32';
  const pathResult = await probeCandidate('ripr', 'path', 'ripr on PATH', probeWithShell, 'unmanaged');
  const resolvedPathResult: ResolvedServer | ResolveFailure =
    isResolved(pathResult) && probeWithShell ? { ...pathResult, needsShell: true } : pathResult;
  if (isResolved(resolvedPathResult)) {
    if (downloadFailure) {
      output.appendLine(`Using PATH fallback after managed server resolution failed: ${downloadFailure}`);
    }
    return resolvedPathResult;
  }

  const autoDownloadHint = config.autoDownload
    ? 'Automatic download was enabled but did not produce a usable server.'
    : 'Automatic download is disabled.';
  return {
    message: 'ripr server is not available.',
    detail: [
      downloadFailure,
      pathResult.detail,
      `${autoDownloadHint} Set ripr.server.path, enable ripr.server.autoDownload, or install with cargo install ripr.`
    ]
      .filter((line): line is string => Boolean(line))
      .join('\n')
  };
}

export function requestedServerVersion(context: vscode.ExtensionContext, config: RiprConfig): string {
  const configured = config.serverVersion.trim();
  if (configured.length > 0) {
    return configured.replace(/^v/, '');
  }
  const version = context.extension?.packageJSON?.version;
  return typeof version === 'string' ? version.replace(/^v/, '') : '0.8.0';
}

function bundledServerPath(context: vscode.ExtensionContext, platform: RiprPlatform): string {
  // Dormant by design (#2085): no platform VSIX ships a bundled server
  // today, so this candidate never exists on disk and resolution falls
  // through to the cache/download path. Kept as the documented first
  // preference for when #1443 / #1624 ship platform VSIXs.
  return path.join(context.extensionUri.fsPath, 'server', platform.target, platform.executableName);
}

async function probeExistingCandidate(
  command: string,
  source: ServerSource,
  detail: string
): Promise<ResolvedServer | ResolveFailure> {
  if (!fs.existsSync(command)) {
    return { message: `${detail} was not found.`, detail: `${command} does not exist.` };
  }
  return probeCandidate(command, source, detail, false, source === 'bundled' ? 'bundled' : 'unmanaged');
}

function probeCandidate(
  command: string,
  source: ServerSource,
  detail: string,
  useShell = false,
  installationState: ResolvedServer['installationState'] = 'unmanaged'
): Promise<ResolvedServer | ResolveFailure> {
  return new Promise((resolve) => {
    const child = cp.spawn(command, ['--version'], { shell: useShell });
    const stdoutChunks: Buffer[] = [];
    const stderrChunks: Buffer[] = [];
    const timer = setTimeout(() => {
      child.kill();
      resolve({
        message: `${detail} did not respond.`,
        detail: `Timed out after ${START_TIMEOUT_MS}ms while running ${command} --version.`
      });
    }, START_TIMEOUT_MS);

    child.stdout?.on('data', (chunk: Buffer) => stdoutChunks.push(chunk));
    child.stderr?.on('data', (chunk: Buffer) => stderrChunks.push(chunk));

    child.once('error', (error) => {
      clearTimeout(timer);
      resolve({ message: `${detail} could not start.`, detail: error.message });
    });

    child.once('exit', (code) => {
      clearTimeout(timer);
      if (code === 0) {
        resolve({
          command,
          source,
          detail,
          binaryVersion: firstOutputLine(stdoutChunks, stderrChunks),
          installationState,
          compatibilityResult: 'not_established'
        });
      } else {
        resolve({ message: `${detail} failed version check.`, detail: `${command} --version exited with code ${code}.` });
      }
    });
  });
}

function withManagedIdentity(
  resolved: ResolvedServer,
  installation: ManagedServerInstallation
): ResolvedServer {
  return {
    ...resolved,
    binaryVersion: installation.receipt.binaryVersion,
    assetDigest: installation.receipt.archiveSha256,
    installationState: 'complete'
  };
}

function isResolved(result: ResolvedServer | ResolveFailure): result is ResolvedServer {
  return 'command' in result;
}

function firstOutputLine(stdoutChunks: Buffer[], stderrChunks: Buffer[]): string | undefined {
  const output = Buffer.concat(stdoutChunks.length > 0 ? stdoutChunks : stderrChunks).toString('utf8');
  return output
    .split(/\r?\n/)
    .map((line) => line.trim())
    .find((line) => line.length > 0);
}

/**
 * The extension's own version, from one source (#14647).
 *
 * VS Code hands the running extension its manifest as
 * `context.extension.packageJSON`. When a context carries no manifest (a
 * hand-built test context, or a host that omits it), the version comes from
 * the `package.json` shipped beside the compiled code, never from a
 * hard-coded release number that drifts as the extension moves on.
 */
import * as fs from 'fs';
import * as path from 'path';
import type * as vscode from 'vscode';

/** `out/src/extensionVersion.js` sits two folders below the manifest, in the repository and in the VSIX. */
const SHIPPED_MANIFEST_PATH = path.join(__dirname, '..', '..', 'package.json');

let shippedVersion: string | null | undefined;

/** The `version` field of the shipped `package.json`, read once; undefined when it cannot be read. */
export function shippedManifestVersion(manifestPath: string = SHIPPED_MANIFEST_PATH): string | undefined {
  if (manifestPath !== SHIPPED_MANIFEST_PATH) {
    return readManifestVersion(manifestPath);
  }
  if (shippedVersion === undefined) {
    shippedVersion = readManifestVersion(manifestPath) ?? null;
  }
  return shippedVersion ?? undefined;
}

function readManifestVersion(manifestPath: string): string | undefined {
  try {
    const manifest: unknown = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
    if (typeof manifest === 'object' && manifest !== null) {
      return nonEmptyVersion((manifest as { version?: unknown }).version);
    }
  } catch {
    // An unreadable manifest leaves the version unknown; callers decide.
  }
  return undefined;
}

function nonEmptyVersion(value: unknown): string | undefined {
  return typeof value === 'string' && value.trim() !== '' ? value.trim().replace(/^v/, '') : undefined;
}

/**
 * The running extension's version: the context's manifest first, then the
 * shipped manifest. Undefined only when neither can be read.
 */
export function knownExtensionVersion(context: vscode.ExtensionContext): string | undefined {
  return nonEmptyVersion(context.extension?.packageJSON?.version) ?? shippedManifestVersion();
}

import * as assert from 'assert';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import * as vscode from 'vscode';
import { RiprConfig } from '../../src/config';
import { knownExtensionVersion, shippedManifestVersion } from '../../src/extensionVersion';
import { requestedServerVersion } from '../../src/serverResolver';
import { ANALYZABLE_PROJECT_MARKERS, extensionVersion } from '../../src/workspaceHelpers';

// Compiled to out/test/suite; the manifest and sources sit three folders up.
const EXTENSION_ROOT = path.resolve(__dirname, '..', '..', '..');

interface Manifest {
  version: string;
  engines: { vscode: string };
  devDependencies: Record<string, string>;
  activationEvents: string[];
}

function manifest(): Manifest {
  return JSON.parse(fs.readFileSync(path.join(EXTENSION_ROOT, 'package.json'), 'utf8')) as Manifest;
}

function contextWithVersion(version?: unknown): vscode.ExtensionContext {
  return (version === undefined ? {} : { extension: { packageJSON: { version } } }) as unknown as vscode.ExtensionContext;
}

suite('Extension version and manifest alignment (#14647)', () => {
  test('the context manifest version wins and a leading v is dropped (#14647)', () => {
    assert.strictEqual(knownExtensionVersion(contextWithVersion('v0.10.0')), '0.10.0');
    assert.strictEqual(extensionVersion(contextWithVersion('0.10.0')), '0.10.0');
  });

  test('a context without a manifest reads the shipped package.json, not a fixed release (#14647)', () => {
    const shipped = manifest().version;
    assert.strictEqual(shippedManifestVersion(), shipped);
    assert.strictEqual(knownExtensionVersion(contextWithVersion()), shipped);
    assert.strictEqual(knownExtensionVersion(contextWithVersion('  ')), shipped);
    assert.strictEqual(extensionVersion(contextWithVersion()), shipped);
    assert.strictEqual(requestedServerVersion(contextWithVersion(), config()), shipped);
  });

  test('a missing or malformed manifest leaves the version unknown (#14647)', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ripr-manifest-version-'));
    try {
      assert.strictEqual(shippedManifestVersion(path.join(root, 'package.json')), undefined);
      const malformed = path.join(root, 'malformed.json');
      fs.writeFileSync(malformed, '{ "version": ');
      assert.strictEqual(shippedManifestVersion(malformed), undefined);
      const noVersion = path.join(root, 'no-version.json');
      fs.writeFileSync(noVersion, '{ "name": "ripr" }');
      assert.strictEqual(shippedManifestVersion(noVersion), undefined);
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  test('only extensionVersion.ts reads the manifest version from a context (#14647)', () => {
    const sourceRoot = path.join(EXTENSION_ROOT, 'src');
    const readers = fs
      .readdirSync(sourceRoot)
      .filter((name) => name.endsWith('.ts'))
      .filter((name) => fs.readFileSync(path.join(sourceRoot, name), 'utf8').includes('packageJSON?.version'));
    assert.deepStrictEqual(readers, ['extensionVersion.ts']);
  });

  test('workspaceContains activation events are exactly the analyzable project markers (#14647)', () => {
    const activationMarkers = manifest()
      .activationEvents.filter((event) => event.startsWith('workspaceContains:'))
      .map((event) => event.slice('workspaceContains:'.length))
      .sort();
    assert.deepStrictEqual(activationMarkers, [...ANALYZABLE_PROJECT_MARKERS].sort());
  });

  test('the manifest declares marker-only activation for both Deno configs (#15649)', () => {
    const events = manifest().activationEvents;
    assert.ok(events.includes('workspaceContains:deno.json'));
    assert.ok(events.includes('workspaceContains:deno.jsonc'));
    assert.ok(!events.includes('workspaceContains:package.json'));
    assert.ok(!events.includes('workspaceContains:settings.jsonc'));
  });

  test('@types/vscode matches the minimum VS Code the extension declares (#14647)', () => {
    const current = manifest();
    const engine = /^\^(\d+)\.(\d+)\.\d+$/.exec(current.engines.vscode);
    const types = /^~(\d+)\.(\d+)\.\d+$/.exec(current.devDependencies['@types/vscode']);
    assert.ok(engine, current.engines.vscode);
    assert.ok(types, current.devDependencies['@types/vscode']);
    assert.deepStrictEqual(types.slice(1, 3), engine.slice(1, 3));
  });
});

function config(): RiprConfig {
  return {
    enabled: true,
    serverPath: '',
    serverArgs: ['lsp', '--stdio'],
    autoDownload: false,
    serverVersion: '',
    downloadBaseUrl: '',
    checkMode: 'draft',
    baseRef: 'origin/main',
    includeUnchangedTests: true,
    seamDiagnostics: true,
    diagnosticProfile: 'actionable',
    traceServer: 'off'
  };
}

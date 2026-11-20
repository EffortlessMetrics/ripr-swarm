//
// `ripr.openReport` locates the report where the server writes it, probes with
// a stat rather than a read, and does not toast on success (#15318).
//
import * as assert from 'assert';
import * as path from 'path';
import * as vscode from 'vscode';
import { RiprClientController, RiprClientRuntime, riprReportsDir } from '../../src/client';
import { RiprConfig } from '../../src/config';

const ROOT = '/workspace';
const REPORTS = path.join(ROOT, 'target', 'ripr', 'reports');

interface Recording {
  infoMessages: string[];
  existsCalls: string[];
  readCalls: string[];
  opened: string[];
}

function controllerWith(existing: string[]): { controller: RiprClientController; rec: Recording } {
  const rec: Recording = { infoMessages: [], existsCalls: [], readCalls: [], opened: [] };
  const runtime = {
    getConfig: () => ({ enabled: true }) as unknown as RiprConfig,
    workspaceRootState: () => ({ kind: 'singleRoot', root: ROOT, roots: [ROOT] }),
    workspaceFolders: () => [],
    readFile: async (p: string) => {
      rec.readCalls.push(p);
      return undefined;
    },
    fileExists: async (p: string) => {
      rec.existsCalls.push(p);
      return existing.includes(p);
    },
    openFileInEditor: async (p: string) => {
      rec.opened.push(p);
    },
    isWorkspaceTrusted: () => true,
    showInformationMessage: (m: string) => {
      rec.infoMessages.push(m);
      return Promise.resolve(undefined);
    },
    showWarningMessage: async () => undefined,
    showErrorMessage: async () => undefined
  } as unknown as RiprClientRuntime;
  const output = { appendLine: () => undefined, show: () => undefined } as unknown as vscode.LogOutputChannel;
  const context = {
    workspaceState: { get: () => undefined, update: () => Promise.resolve() }
  } as unknown as vscode.ExtensionContext;
  const controller = new RiprClientController(context, output, runtime);
  // The root is normally bound at start; bind it directly so no server starts.
  (controller as unknown as { workspaceRoot: string }).workspaceRoot = ROOT;
  return { controller, rec };
}

suite('ripr.openReport (#15318)', () => {
  test('opens an existing report with no toast and never reads the file', async () => {
    const summary = path.join(REPORTS, 'pr-evidence-summary.md');
    const { controller, rec } = controllerWith([summary]);
    await controller.openReport();
    assert.deepStrictEqual(rec.opened, [summary]);
    assert.deepStrictEqual(rec.infoMessages, [], 'a successful open must not toast');
    assert.deepStrictEqual(rec.readCalls, [], 'existence is a stat, not a full read');
  });

  test('falls back to start-here.md when the summary is absent', async () => {
    const startHere = path.join(REPORTS, 'start-here.md');
    const { controller, rec } = controllerWith([startHere]);
    await controller.openReport();
    assert.deepStrictEqual(rec.opened, [startHere]);
  });

  test('a custom cargo target dir does not redirect: the server writes under <root>/target/ripr', async () => {
    const saved = process.env.CARGO_TARGET_DIR;
    process.env.CARGO_TARGET_DIR = '/elsewhere/target';
    try {
      const elsewhere = '/elsewhere/target/ripr/reports/pr-evidence-summary.md';
      const { controller, rec } = controllerWith([elsewhere]);
      await controller.openReport();
      assert.deepStrictEqual(rec.opened, [], 'the server never writes there, so it is not a report location');
      assert.ok(rec.existsCalls.every((p) => p.startsWith(REPORTS)), rec.existsCalls.join(','));
    } finally {
      if (saved === undefined) {
        delete process.env.CARGO_TARGET_DIR;
      } else {
        process.env.CARGO_TARGET_DIR = saved;
      }
    }
  });

  test('a missing report names where it looked and how to produce one', async () => {
    const { controller, rec } = controllerWith([]);
    await controller.openReport();
    assert.deepStrictEqual(rec.opened, []);
    assert.strictEqual(rec.infoMessages.length, 1);
    assert.ok(rec.infoMessages[0].includes(riprReportsDir(ROOT)), rec.infoMessages[0]);
    assert.ok(rec.infoMessages[0].includes('ripr pr-summary'), rec.infoMessages[0]);
  });
});

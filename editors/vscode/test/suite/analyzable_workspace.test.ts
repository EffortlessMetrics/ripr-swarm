import * as assert from 'assert';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import * as vscode from 'vscode';
import {
  workspaceHasAnalyzableProject,
} from '../../src/workspaceHelpers';
import {
  resetLifecycleCoordinatorForTests,
  startServerOnActivationIfAnalyzable,
} from '../../src/extension';

function tempWorkspaceDir(prefix: string): string {
  return fs.mkdtempSync(path.join(os.tmpdir(), prefix));
}

function workspaceFolder(root: string, index = 0): vscode.WorkspaceFolder {
  return {
    uri: vscode.Uri.file(root),
    name: path.basename(root),
    index,
  };
}

async function writeMarker(root: string, marker: string): Promise<void> {
  const filePath = path.join(root, marker);
  if (marker.endsWith('.toml') || marker.endsWith('.ini') || marker.endsWith('.cfg')) {
    fs.writeFileSync(filePath, '# fixture\n');
  } else if (marker.endsWith('.py')) {
    fs.writeFileSync(filePath, '# fixture\n');
  } else {
    fs.writeFileSync(filePath, 'fixture\n');
  }
}

suite('Analyzable workspace markers', () => {
  test('Cargo.toml at a folder root is analyzable', async () => {
    const root = tempWorkspaceDir('ripr-analyzable-rust-');
    try {
      await writeMarker(root, 'Cargo.toml');
      assert.strictEqual(await workspaceHasAnalyzableProject([workspaceFolder(root)]), true);
    } finally {
      fs.rmSync(root, { force: true, recursive: true });
    }
  });

  test('ripr.toml at a folder root is analyzable', async () => {
    const root = tempWorkspaceDir('ripr-analyzable-ripr-');
    try {
      await writeMarker(root, 'ripr.toml');
      assert.strictEqual(await workspaceHasAnalyzableProject([workspaceFolder(root)]), true);
    } finally {
      fs.rmSync(root, { force: true, recursive: true });
    }
  });

  test('a Python project marker at the folder root is analyzable', async () => {
    const root = tempWorkspaceDir('ripr-analyzable-python-');
    try {
      await writeMarker(root, 'pyproject.toml');
      assert.strictEqual(await workspaceHasAnalyzableProject([workspaceFolder(root)]), true);
    } finally {
      fs.rmSync(root, { force: true, recursive: true });
    }
  });

  test('a TypeScript-only folder without markers is not analyzable', async () => {
    const root = tempWorkspaceDir('ripr-analyzable-bare-ts-');
    try {
      fs.mkdirSync(path.join(root, 'src'), { recursive: true });
      fs.writeFileSync(path.join(root, 'src', 'index.ts'), 'export const x = 1;\n');
      assert.strictEqual(await workspaceHasAnalyzableProject([workspaceFolder(root)]), false);
    } finally {
      fs.rmSync(root, { force: true, recursive: true });
    }
  });

  test('a directory named like a Python marker is not analyzable', async () => {
    const root = tempWorkspaceDir('ripr-analyzable-marker-dir-');
    try {
      fs.mkdirSync(path.join(root, 'pyproject.toml'), { recursive: true });
      assert.strictEqual(await workspaceHasAnalyzableProject([workspaceFolder(root)]), false);
    } finally {
      fs.rmSync(root, { force: true, recursive: true });
    }
  });

  test('multi-root windows are analyzable when any folder has a marker', async () => {
    const bare = tempWorkspaceDir('ripr-analyzable-mixed-bare-');
    const rust = tempWorkspaceDir('ripr-analyzable-mixed-rust-');
    try {
      fs.writeFileSync(path.join(bare, 'app.js'), 'module.exports = {};\n');
      await writeMarker(rust, 'Cargo.toml');
      assert.strictEqual(
        await workspaceHasAnalyzableProject([workspaceFolder(bare, 0), workspaceFolder(rust, 1)]),
        true
      );
    } finally {
      fs.rmSync(bare, { force: true, recursive: true });
      fs.rmSync(rust, { force: true, recursive: true });
    }
  });
});

suite('Activation server start gate', () => {
  setup(() => {
    resetLifecycleCoordinatorForTests();
  });

  test('activation start stays idle in a bare TypeScript workspace folder', async () => {
    const root = tempWorkspaceDir('ripr-activation-bare-ts-');
    const lines: string[] = [];
    const output = { appendLine: (line: string) => { lines.push(line); } } as unknown as vscode.LogOutputChannel;
    let starts = 0;
    const controller = {
      start: async () => {
        starts += 1;
      },
    };
    try {
      fs.mkdirSync(path.join(root, 'src'), { recursive: true });
      fs.writeFileSync(path.join(root, 'src', 'index.ts'), 'export const value = 1;\n');
      await startServerOnActivationIfAnalyzable(controller, output, [workspaceFolder(root)]);
      assert.strictEqual(starts, 0, 'bare TS folder must not start the server on activation');
      assert.ok(
        lines.some((line) => line.includes('staying idle')),
        `expected idle log line, got: ${lines.join(' | ')}`
      );
    } finally {
      fs.rmSync(root, { force: true, recursive: true });
    }
  });

  test('activation start runs when a folder has Cargo.toml', async () => {
    const root = tempWorkspaceDir('ripr-activation-rust-');
    const output = { appendLine: () => undefined } as unknown as vscode.LogOutputChannel;
    let starts = 0;
    const controller = {
      start: async () => {
        starts += 1;
      },
    };
    try {
      await writeMarker(root, 'Cargo.toml');
      await startServerOnActivationIfAnalyzable(controller, output, [workspaceFolder(root)]);
      assert.strictEqual(starts, 1);
    } finally {
      fs.rmSync(root, { force: true, recursive: true });
    }
  });
});

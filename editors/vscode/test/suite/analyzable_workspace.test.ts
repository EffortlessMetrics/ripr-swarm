import * as assert from 'assert';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import * as vscode from 'vscode';
import {
  NESTED_PROJECT_MARKER_MAX_DEPTH,
  NESTED_PROJECT_MARKER_MAX_DIRS,
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

function writeNestedFile(root: string, relativePath: string): void {
  const filePath = path.join(root, ...relativePath.split('/'));
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, '# fixture\n');
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

  test('tsconfig.json at the folder root is analyzable', async () => {
    const root = tempWorkspaceDir('ripr-analyzable-ts-');
    try {
      await writeMarker(root, 'tsconfig.json');
      assert.strictEqual(await workspaceHasAnalyzableProject([workspaceFolder(root)]), true);
    } finally {
      fs.rmSync(root, { force: true, recursive: true });
    }
  });

  test('jsconfig.json at the folder root is analyzable', async () => {
    const root = tempWorkspaceDir('ripr-analyzable-js-');
    try {
      await writeMarker(root, 'jsconfig.json');
      assert.strictEqual(await workspaceHasAnalyzableProject([workspaceFolder(root)]), true);
    } finally {
      fs.rmSync(root, { force: true, recursive: true });
    }
  });

  test('package.json alone is not analyzable', async () => {
    const root = tempWorkspaceDir('ripr-analyzable-pkgjson-');
    try {
      await writeMarker(root, 'package.json');
      assert.strictEqual(await workspaceHasAnalyzableProject([workspaceFolder(root)]), false);
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

  test('a project marker a few folders below the root is analyzable', async () => {
    for (const marker of ['backend/Cargo.toml', 'packages/app/tsconfig.json', 'services/api/v1/pyproject.toml']) {
      const root = tempWorkspaceDir('ripr-analyzable-nested-');
      try {
        writeNestedFile(root, marker);
        assert.strictEqual(await workspaceHasAnalyzableProject([workspaceFolder(root)]), true, marker);
      } finally {
        fs.rmSync(root, { force: true, recursive: true });
      }
    }
  });

  test('nested markers outside the bounded project search are not analyzable', async () => {
    const cases = [
      // Deeper than the search goes.
      Array.from({ length: NESTED_PROJECT_MARKER_MAX_DEPTH + 1 }, (_, level) => `l${level}`).join('/') + '/Cargo.toml',
      // Dependency, build, environment and hidden folders.
      'node_modules/dep/tsconfig.json',
      'target/package/crate-0.1.0/Cargo.toml',
      'vendor/crate/Cargo.toml',
      'venv/lib/pyproject.toml',
      '.cache/tool/Cargo.toml',
      // Root-only markers: tooling files in a project of another language.
      'docs/requirements.txt',
      'tools/noxfile.py',
      'examples/ripr.toml',
      // Not a marker at any level.
      'web/package.json',
    ];
    for (const marker of cases) {
      const root = tempWorkspaceDir('ripr-analyzable-nested-miss-');
      try {
        writeNestedFile(root, marker);
        assert.strictEqual(await workspaceHasAnalyzableProject([workspaceFolder(root)]), false, marker);
      } finally {
        fs.rmSync(root, { force: true, recursive: true });
      }
    }
  });

  test('a nested folder named like a marker is not analyzable', async () => {
    const root = tempWorkspaceDir('ripr-analyzable-nested-dir-');
    try {
      fs.mkdirSync(path.join(root, 'backend', 'Cargo.toml'), { recursive: true });
      assert.strictEqual(await workspaceHasAnalyzableProject([workspaceFolder(root)]), false);
    } finally {
      fs.rmSync(root, { force: true, recursive: true });
    }
  });

  test('the nested project search stops at its folder budget', async () => {
    const root = tempWorkspaceDir('ripr-analyzable-nested-budget-');
    try {
      // More first-level folders than the read budget, counting the root's
      // own read: the budget runs out before any second-level folder is read,
      // in whatever order the folders are listed.
      for (let index = 0; index < NESTED_PROJECT_MARKER_MAX_DIRS; index++) {
        fs.mkdirSync(path.join(root, `a${String(index).padStart(4, '0')}`));
      }
      writeNestedFile(root, 'zz/inner/Cargo.toml');
      assert.strictEqual(await workspaceHasAnalyzableProject([workspaceFolder(root)]), false);
    } finally {
      fs.rmSync(root, { force: true, recursive: true });
    }
  });

  test('a wide sibling does not use up the budget before a queued marker folder is read', async () => {
    const root = tempWorkspaceDir('ripr-analyzable-nested-wide-');
    try {
      // `assets` lists before `backend` and holds more folders than the
      // budget. Queuing them must not stop the walk before `backend` is read.
      for (let index = 0; index <= NESTED_PROJECT_MARKER_MAX_DIRS; index++) {
        fs.mkdirSync(path.join(root, 'assets', `a${String(index).padStart(4, '0')}`), { recursive: true });
      }
      writeNestedFile(root, 'backend/Cargo.toml');
      assert.strictEqual(await workspaceHasAnalyzableProject([workspaceFolder(root)]), true);
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

  test('activation start runs when the Cargo.toml is below the folder root', async () => {
    const root = tempWorkspaceDir('ripr-activation-nested-rust-');
    const output = { appendLine: () => undefined } as unknown as vscode.LogOutputChannel;
    let starts = 0;
    const controller = {
      start: async () => {
        starts += 1;
      },
    };
    try {
      writeNestedFile(root, 'backend/Cargo.toml');
      await startServerOnActivationIfAnalyzable(controller, output, [workspaceFolder(root)]);
      assert.strictEqual(starts, 1);
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

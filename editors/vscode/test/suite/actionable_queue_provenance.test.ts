import * as assert from 'assert';
import { createHash } from 'crypto';
import { promises as fs } from 'fs';
import * as path from 'path';
import * as vscode from 'vscode';
import { RiprClientController, RiprClientRuntime, readActionableGapQueueStatus } from '../../src/client';
import { RiprConfig } from '../../src/config';
import { compatibleLspEvidence } from './testCompatibility';

const queueFile = 'target/ripr/reports/actionable-gaps.json';
const sourceFile = 'crates/ordinary_shop/src/lib.rs';
const testFile = 'crates/ordinary_shop/tests/pricing.rs';
const gapId = 'gap:ba9f4264c3029d1c';
const boundedVerify = 'cargo test -p ordinary_shop above_threshold_discounts';
type Queue = { [key: string]: any; packets: Array<Record<string, any>> };

// The fixture comes from the retained public Rust producer/projector test.
// No handwritten positive queue or changed provenance/command supplies it.
suite('Genuine actionable queue consumer', () => {
  let root: string;
  let original: string;
  let genuine: Queue;

  suiteSetup(async function () {
    if (process.env.RIPR_TEST_WORKSPACE_TRUST === 'untrusted') {
      this.skip();
      return;
    }
    const suppliedRoot = process.env.RIPR_TEST_ACTIONABLE_QUEUE_ROOT;
    if (!suppliedRoot) {
      console.info('Genuine queue composition not established: producer fixture root was not supplied.');
      this.skip();
      return;
    }
    root = vscode.Uri.file(path.resolve(suppliedRoot)).fsPath;
    original = await fs.readFile(path.join(root, queueFile), 'utf8');
    genuine = JSON.parse(original) as Queue;
    assert.strictEqual(genuine.root, undefined, 'the actual projector emits a workspace-local queue without a root hint');
    assert.strictEqual(genuine.packets.length, 1);
    const packet = genuine.packets[0];
    assert.strictEqual(packet.canonical_gap_id, gapId);
    assert.strictEqual(packet.source_file, sourceFile);
    assert.strictEqual(packet.verify_command, boundedVerify);
    assert.strictEqual(packet.verify_command_source, 'related_test_or_observer.name');
    assert.strictEqual(packet.public_projection_eligible, true);
    assert.deepStrictEqual(packet.projection_exclusion_reasons, []);
    assert.strictEqual(packet.related_test_or_observer.file, testFile);
    assert.strictEqual(packet.related_test_or_observer.name, 'above_threshold_discounts');
    assert.ok(packet.raw_findings.length > 0);
    assert.strictEqual(genuine.source_subject.corpus.scope, 'rust_workspace');
    for (const file of [sourceFile, testFile]) {
      const bytes = await fs.readFile(path.join(root, file));
      const expected = genuine.source_subject.files.find((entry: any) => entry.path === file);
      assert.strictEqual(expected?.digest, `sha256:${createHash('sha256').update(bytes).digest('hex')}`);
    }
  });

  test('unchanged genuine bytes reach the setup-files queue reader', async () => {
    const reads: string[] = [];
    const status = await readActionableGapQueueStatus(root, fileReader(reads));
    assert.ok(reads.includes(path.join(root, queueFile)));
    assert.strictEqual(status.state, 'topActionableGap');
    assert.strictEqual(status.canonicalGapId, gapId);
    assert.strictEqual(status.verifyCommand, boundedVerify);
    assert.strictEqual(status.relatedTest, testFile);
  });

  test('Copy Current Repair Packet uses real setup-file reads without RPC or CLI shortcuts', async () => {
    const h = controllerHarness();
    try {
      await h.controller.start(); h.reads.length = 0;
      await h.controller.copyCurrentRepairPacket();
      assert.ok(h.reads.includes(path.join(root, queueFile)));
      assert.strictEqual(h.copies.length, 1);
      const copied = h.copies[0];
      for (const line of [
        'RIPR current repair packet', `Repair this one canonical gap: ${gapId}.`,
        `Source: ${sourceFile}`, `Related test: ${testFile}`, `Run: ${boundedVerify}`,
        'Missing discriminator: threshold (equality boundary)',
        'Repair kind: add_boundary_assertion', 'Confidence basis: static_only',
        'Do not treat this advisory static packet as a gate decision'
      ]) { assert.ok(copied.includes(line), `${line}\n${copied}`); }
      assert.ok(h.infos.at(-1)?.includes('current repair packet copied'));
      assert.strictEqual(h.requests.length, 0);
      assert.strictEqual(h.cliCalls.length, 0);
    } finally { await h.controller.stop(); }
    assert.strictEqual(await fs.readFile(path.join(root, queueFile), 'utf8'), original);
  });

  test('Copy Current Repair Packet refuses actual unsaved controller state', async () => {
    const h = controllerHarness();
    try {
      await h.controller.start();
      h.controller.markWorkspaceStale({ uri: vscode.Uri.file(path.join(root, sourceFile)), languageId: 'rust' } as vscode.TextDocument);
      await h.controller.copyCurrentRepairPacket();
      assert.deepStrictEqual(h.copies, []);
      assert.deepStrictEqual(h.requests, []);
      assert.deepStrictEqual(h.cliCalls, []);
    } finally { await h.controller.stop(); }
  });

  const controls: Array<[string, (queue: Queue) => void]> = [
    ['unknown provenance', q => { q.packets[0].verify_command_source = 'recommendation'; }],
    ['missing provenance', q => { delete q.packets[0].verify_command_source; }],
    ['wrong root', q => { q.root = path.join(root, 'another-root'); }],
    ['declared stale queue', q => { q.status = 'stale'; }],
    ['declared stale editor evidence', q => { q.editor_context = { freshness: 'stale' }; }],
    ['producer run limitation', q => { q.run_limitations = [{ reason: 'incomplete' }]; }],
    ['unsupported schema', q => { q.schema_version = 'unknown'; }],
    ['parent source path', q => { q.packets[0].source_file = '../outside.rs'; }],
    ['parent related-test path', q => { q.packets[0].related_test_or_observer.file = '../outside.rs'; }],
    ['unknown package', q => { q.packets[0].verify_command = 'cargo test -p another above_threshold_discounts'; }],
    ['wrong filter', q => { q.packets[0].verify_command = 'cargo test -p ordinary_shop another_test'; }],
    ['extra Cargo option', q => { q.packets[0].verify_command += ' --manifest-path ../Cargo.toml'; }],
    ['unbounded Cargo suite', q => { q.packets[0].verify_command = 'cargo test -p ordinary_shop'; }],
    ['option-shaped filter', q => {
      q.packets[0].related_test_or_observer.name = '--all-features';
      q.packets[0].verify_command = 'cargo test -p ordinary_shop --all-features';
    }],
    ['workspace option-shaped filter', q => {
      q.packets[0].related_test_or_observer.name = '--workspace';
      q.packets[0].verify_command = 'cargo test -p ordinary_shop --workspace';
    }],
    ['option-shaped package', q => {
      q.packets[0].related_test_or_observer.file = 'crates/--workspace/tests/pricing.rs';
      q.packets[0].verify_command = 'cargo test -p --workspace above_threshold_discounts';
    }],
    ['Cargo verification copied into receipt role', q => {
      q.packets[0].receipt_command_or_path = boundedVerify;
    }],
    ['shell suffix', q => { q.packets[0].verify_command += '; echo injected'; }],
    ['raw newline', q => {
      q.packets[0].verify_command_source = 'canonical_item.verify_command';
      q.packets[0].verify_command = 'cargo xtask fixtures\nboundary_gap';
    }],
    ['raw carriage return', q => {
      q.packets[0].verify_command_source = 'canonical_item.verify_command';
      q.packets[0].verify_command = 'cargo xtask fixtures\rboundary_gap';
    }],
    ['broad snapshot comparison', q => { q.packets[0].verify_command = 'ripr agent verify --before repo-exposure.json --after after.repo-exposure.json'; }],
    ['quoted-fragment broad comparison', q => {
      q.packets[0].verify_command = "ripr agent verify --'bef'ore repo-exposure.json --a'ft'er after.repo-exposure.json";
    }],
    ['canonical broad snapshot comparison', q => {
      q.packets[0].verify_command_source = 'canonical_item.verify_command';
      q.packets[0].verify_command = 'ripr agent verify --before repo-exposure.json --after after.repo-exposure.json';
    }],
    ['canonical quoted-fragment broad comparison', q => {
      q.packets[0].verify_command_source = 'canonical_item.verify_command';
      q.packets[0].verify_command = "ripr agent verify --'bef'ore repo-exposure.json --a'ft'er after.repo-exposure.json";
    }],
    ['false eligibility', q => { q.packets[0].public_projection_eligible = false; }],
    ['projection exclusion', q => { q.packets[0].projection_exclusion_reasons = ['unbounded_verify_command']; }],
    ['malformed exclusions', q => { q.packets[0].projection_exclusion_reasons = {}; }],
    ['unknown actionable sibling', q => {
      const sibling = structuredClone(q.packets[0]); sibling.verify_command_source = 'unknown'; q.packets.push(sibling);
    }],
    ['excluded actionable sibling', q => {
      const sibling = structuredClone(q.packets[0]); sibling.projection_exclusion_reasons = ['suppressed']; q.packets.push(sibling);
    }],
    ['unsafe actionable sibling', q => {
      const sibling = structuredClone(q.packets[0]); sibling.receipt_command_or_path = '../outside.json'; q.packets.push(sibling);
    }],
    ['actionable sibling missing identity', q => {
      const sibling = structuredClone(q.packets[0]); delete sibling.canonical_gap_id; q.packets.push(sibling);
    }],
    ['actionable sibling missing receipt', q => {
      const sibling = structuredClone(q.packets[0]); delete sibling.receipt_command_or_path; q.packets.push(sibling);
    }],
    ['canonical actionable sibling missing verification', q => {
      const sibling = structuredClone(q.packets[0]);
      sibling.verify_command_source = 'canonical_item.verify_command'; delete sibling.verify_command; q.packets.push(sibling);
    }]
  ];

  for (const [name, mutate] of controls) {
    test(`Copy Current Repair Packet refuses ${name}`, async () => {
      const changed = structuredClone(genuine); mutate(changed);
      if (changed.packets.length === 2) {
        Object.assign(changed.summary, { actionable_gaps: 2, canonical_items: 2,
          packets_emitted: 2, public_projection_eligible_packets: 2, raw_signals: 2 });
      }
      const h = controllerHarness(JSON.stringify(changed));
      try {
        await h.controller.start(); h.reads.length = 0;
        await h.controller.copyCurrentRepairPacket();
        assert.ok(h.reads.includes(path.join(root, queueFile)));
        assert.deepStrictEqual(h.copies, [], 'invalid evidence must not copy a repair packet');
        assert.ok(h.infos.at(-1)?.includes('suppressed') || h.infos.at(-1)?.includes('stale'));
        assert.deepStrictEqual(h.requests, []);
        assert.deepStrictEqual(h.cliCalls, []);
      } finally { await h.controller.stop(); }
      assert.strictEqual(await fs.readFile(path.join(root, queueFile), 'utf8'), original);
    });
  }

  function fileReader(reads: string[], override?: string): RiprClientRuntime['readFile'] {
    return async file => {
      reads.push(file);
      let text: string;
      try { text = await fs.readFile(file, 'utf8'); }
      catch (error) { if ((error as NodeJS.ErrnoException).code === 'ENOENT') { return undefined; } throw error; }
      return file === path.join(root, queueFile) && override !== undefined ? override : text;
    };
  }

  function controllerHarness(override?: string) {
    const reads: string[] = [], copies: string[] = [], infos: string[] = [];
    const requests: string[] = [], cliCalls: string[] = [];
    const config: RiprConfig = {
      enabled: true, serverPath: '/sentinel/queue/ripr', serverArgs: ['lsp', '--stdio'],
      autoDownload: false, serverVersion: '0.11.0', downloadBaseUrl: 'https://sentinel.invalid',
      checkMode: 'draft', baseRef: 'HEAD', includeUnchangedTests: true,
      seamDiagnostics: true, diagnosticProfile: 'full', traceServer: 'off'
    };
    const folder = { name: 'genuine-queue', index: 0, uri: vscode.Uri.file(root) };
    const runtime: RiprClientRuntime = {
      getConfig: () => config,
      workspaceRootState: () => ({ kind: 'singleRoot', root, roots: [root] }),
      workspaceFolders: () => [folder], showQuickPick: async () => undefined,
      resolveServer: async () => ({ command: config.serverPath, source: 'configured', detail: 'controller proof only',
        installationState: 'unmanaged', compatibilityResult: compatibleLspEvidence }),
      createLanguageClient: () => ({
        onNotification: () => ({ dispose: () => undefined }), setTrace: () => undefined,
        start: async () => undefined, stop: async () => undefined,
        sendRequest: async method => { requests.push(method); throw new Error('queue proof must not use an LSP shortcut'); }
      }),
      createFileSystemWatcher: () => ({} as vscode.FileSystemWatcher),
      readFile: fileReader(reads, override),
      runRipr: async (_command, args) => { cliCalls.push(args.join(' ')); throw new Error('queue proof must not use a CLI shortcut'); },
      writeClipboard: async text => { copies.push(text); }, isWorkspaceTrusted: () => true,
      showInformationMessage: async message => { infos.push(message); return undefined; },
      showWarningMessage: async () => undefined, showErrorMessage: async () => undefined
    };
    const controller = new RiprClientController({} as vscode.ExtensionContext,
      { appendLine: () => undefined } as unknown as vscode.LogOutputChannel, runtime);
    return { controller, reads, copies, infos, requests, cliCalls };
  }
});

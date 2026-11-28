import * as assert from 'assert';
import { createHash } from 'crypto';
import { promises as fs } from 'fs';

export const REPAIR_START_OBSERVATION = 'repair-start-observation.json';
export const REPAIR_START_EVIDENCE = 'source_harness_observation';
export const REPAIR_START_ADMISSION = 'not_established';
export const REPAIR_START_GAP = 'gap:rust:src/lib.rs:loyalty_price:predicate_boundary:predicate:member_years==5';
export const REPAIR_START_TARGET = 'tests/loyalty_price.rs';

export interface RepairStartRun {
  case: 'called' | 'uncalled';
  nonce: string;
  output: string;
  workspace: string;
  head: string;
  base: string;
  extension: string;
  server: string;
  serverSha256: string;
}

export async function assertResolvedRepairStartServer(run: RepairStartRun, server: {
  source: string; command: string; needsShell?: boolean;
} | undefined, running: boolean): Promise<void> {
  assert.ok(running && server, 'the observed production LSP client must be running');
  assert.strictEqual(server.source, 'configured');
  assert.ok(!server.needsShell, 'the observed LSP server must launch directly');
  assert.strictEqual(server.command, run.server, 'the observed LSP server must resolve to the pinned executable');
  const resolved = await fs.realpath(server.command);
  assert.strictEqual(resolved, run.server, 'the observed LSP server must resolve to the pinned executable');
  const digest = createHash('sha256').update(await fs.readFile(resolved)).digest('hex');
  assert.strictEqual(digest, run.serverSha256, 'the observed LSP server must match the pinned digest');
}

export async function inspectRepairStartCard(
  run: RepairStartRun,
  copy: () => Promise<string>
): Promise<{ seam: string; targetKind: string }> {
  const card = JSON.parse(await copy());
  assert.strictEqual(card.subject.canonical_gap_id, REPAIR_START_GAP);
  assert.strictEqual(card.snapshot.currentness, 'current');
  assert.strictEqual(card.snapshot.repository_head, run.head, 'RepairCard must describe the staged fixture HEAD');
  assert.strictEqual(card.selected_target.file, REPAIR_START_TARGET);
  const targetKind: unknown = card.selected_target.kind;
  assert.ok(targetKind === 'existing' || targetKind === 'proposed');
  if (run.case === 'called') {
    assert.strictEqual(targetKind, 'proposed', 'the called case must expose a proposed test target');
    assert.strictEqual(card.instruction.state, 'static_limitation');
    assert.strictEqual(card.next_action, null);
  }
  const seam: unknown = card.subject.seam_id;
  assert.ok(typeof seam === 'string' && seam.length > 0);
  return { seam, targetKind };
}

export function validateRepairStartObservation(run: RepairStartRun, observation: any): void {
  assert.strictEqual(observation.nonce, run.nonce, 'repair Start observation is from a different invocation');
  assert.strictEqual(observation.evidence, REPAIR_START_EVIDENCE);
  assert.strictEqual(observation.package_admission, REPAIR_START_ADMISSION);
  assert.strictEqual(observation.case, run.case);
  assert.strictEqual(observation.fixture_root, run.workspace);
  assert.strictEqual(observation.repository_head, run.head);
  assert.strictEqual(observation.repository_base, run.base);
  assert.strictEqual(observation.extension_path, run.extension);
  assert.strictEqual(observation.server, run.server);
  assert.strictEqual(observation.server_sha256, run.serverSha256);
  assert.strictEqual(observation.gap, REPAIR_START_GAP);
  assert.strictEqual(observation.target, REPAIR_START_TARGET);
  assert.strictEqual(observation.currentness, 'current');
  assert.ok(observation.target_kind === 'existing' || observation.target_kind === 'proposed');
  assert.strictEqual(observation.platform, process.platform);
  assert.strictEqual(observation.arch, process.arch);
  for (const field of ['server_version', 'extension_version', 'vscode_version', 'node_version']) {
    const value: unknown = observation[field];
    assert.ok(typeof value === 'string' && value.trim() !== '', `repair Start observation requires ${field}`);
  }
  assert.ok(typeof observation.seam === 'string' && observation.seam.length > 0);
  assert.strictEqual(observation.enabled_start, run.case === 'called' ? 0 : 1);
  if (run.case === 'called') {
    assert.strictEqual(observation.target_kind, 'proposed');
    assert.strictEqual(observation.copied_start, null);
  } else {
    assert.ok(typeof observation.copied_start === 'string'
      && observation.copied_start.startsWith('ripr agent repair --root ')
      && observation.copied_start.endsWith(` --seam-id ${observation.seam} --phase before`));
  }
}

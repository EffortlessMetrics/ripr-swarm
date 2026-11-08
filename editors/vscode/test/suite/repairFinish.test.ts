//
// Shared repair after-phase parser and live-attempt selection (#4647, #2928).
//

import * as assert from 'assert';
import { promises as fs } from 'fs';
import * as path from 'path';
import {
  buildAgentRepairAfterArgs,
  liveRepairAttemptRows,
  parseAfterPhaseRepairFinish,
  resolveLiveAttemptForFinish,
} from '../../src/repairFinish';
import { parseAttemptInventory } from '../../src/attemptStatus';

const FIXTURE_DIR = path.resolve(__dirname, '../../../test-fixtures/attempt-status');

suite('Repair finish helpers (#4647)', () => {
  test('buildAgentRepairAfterArgs pins root, attempt, and json', () => {
    assert.deepStrictEqual(
      buildAgentRepairAfterArgs('/repo', 'repair-attempt-0123456789abcdef01234567'),
      [
        'agent',
        'repair',
        '--root',
        '/repo',
        '--attempt',
        'repair-attempt-0123456789abcdef01234567',
        '--phase',
        'after',
        '--json',
      ]
    );
  });

  test('live rows filter to resumable awaiting_edit with an after command', async () => {
    const inventory = await loadInventory();
    assert.ok(inventory);
    const live = liveRepairAttemptRows(inventory);
    assert.ok(live.length >= 1);
    for (const row of live) {
      assert.ok(row.state === 'awaiting_edit' || row.state === 'prepared');
      assert.strictEqual(row.disposition, 'resumable');
      assert.ok(row.command?.includes('--phase after'));
    }
  });

  test('resolveLiveAttemptForFinish requires explicit pick for several live attempts', async () => {
    const inventory = await loadInventory();
    assert.ok(inventory);
    const resolution = resolveLiveAttemptForFinish(inventory);
    if (liveRepairAttemptRows(inventory).length > 1) {
      assert.strictEqual(resolution.kind, 'selection_required');
    }
  });

  test('Continue excludes retained baseline limits even with an old after command (#13216)', () => {
    const rows = parseAttemptInventory({
      schema_version: '0.1',
      repair_attempts: [
        { attempt_id: 'ambiguous', state: 'awaiting_edit', disposition: 'baseline_ambiguous', baseline_ambiguous: true, command: 'ripr agent repair --attempt ambiguous --phase after' },
        { attempt_id: 'unavailable', state: 'awaiting_edit', disposition: 'baseline_unavailable', baseline_ambiguous: null, baseline_unavailable_reason: 'retained baseline cannot be read', command: 'ripr agent repair --attempt unavailable --phase after' }
      ]
    });
    assert.ok(rows);
    assert.deepStrictEqual(liveRepairAttemptRows(rows), []);
    assert.deepStrictEqual(resolveLiveAttemptForFinish(rows, { rememberedAttemptId: 'ambiguous' }), { kind: 'no_attempts' });
    assert.deepStrictEqual(resolveLiveAttemptForFinish(rows, { explicitAttemptId: 'ambiguous' }), { kind: 'unknown_attempt', attemptId: 'ambiguous' });
    assert.deepStrictEqual(liveRepairAttemptRows([
      ...rows,
      { attemptId: 'clean', state: 'awaiting_edit', disposition: 'resumable', command: 'ripr agent repair --attempt clean --phase after' }
    ]).map((row) => row.attemptId), ['clean']);
  });

  test('parseAfterPhaseRepairFinish reads repair_after_refusal without strengthening', () => {
    const parsed = parseAfterPhaseRepairFinish(
      JSON.stringify({
        schema_version: '0.2',
        kind: 'repair_after_refusal',
        attempt_id: 'repair-attempt-deadbeefdeadbeefdeadbeefdeadbeef',
        error: 'repair attempt cannot finish: HEAD moved',
        narration: ['HEAD diverged from the before-phase head.', 'Check out the prepared head or start anew.'],
      })
    );
    assert.ok(parsed);
    assert.strictEqual(parsed?.refusalError, 'repair attempt cannot finish: HEAD moved');
    assert.strictEqual(parsed?.summaryLines.length, 2);
  });

  test('parseAfterPhaseRepairFinish reads repair_after_result envelope', async function (this: Mocha.Context) {
    const statusFixture = await fs.readFile(
      path.join(FIXTURE_DIR, 'status-finished_current.json'),
      'utf8'
    ).catch(() => undefined);
    if (!statusFixture) {
      this.skip();
      return;
    }
    const agentStatus = JSON.parse(statusFixture) as unknown;
    const parsed = parseAfterPhaseRepairFinish(
      JSON.stringify({
        schema_version: '0.1',
        kind: 'repair_after_result',
        verify: {
          schema_version: '0.3',
          tool: 'ripr',
          movement: { summary: 'Result for seam demo: weak -> exposed (improved).' },
        },
        agent_status: agentStatus,
      })
    );
    assert.ok(parsed?.agentStatus);
    assert.ok(parsed?.summaryLines.some((line) => line.includes('improved')));
  });

  test('parseAfterPhaseRepairFinish accepts a lone agent_attempt_status (already complete)', async () => {
    const raw = await fs.readFile(path.join(FIXTURE_DIR, 'status-awaiting_edit.json'), 'utf8');
    const parsed = parseAfterPhaseRepairFinish(raw);
    assert.ok(parsed?.agentStatus);
    assert.strictEqual(parsed?.agentStatus?.statusClass, 'awaiting_edit');
  });
});

async function loadInventory() {
  const raw = await fs.readFile(path.join(FIXTURE_DIR, 'inventory.json'), 'utf8');
  return parseAttemptInventory(JSON.parse(raw) as unknown);
}

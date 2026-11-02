import * as assert from 'assert';
import {
  buildAgentRepairBeforeArgs,
  extractAgentRepairLoopTarget,
  findLiveRepairAttemptForSeam,
  parseBeforePhaseRepairStart,
  seamIdFromAgentRepairTarget,
} from '../../src/repairStart';
import type { AttemptInventoryRow } from '../../src/attemptStatus';

suite('repairStart (#4645)', () => {
  test('buildAgentRepairBeforeArgs pins the shared before phase', () => {
    assert.deepStrictEqual(
      buildAgentRepairBeforeArgs('/workspace', '67fc764ba37d77bd'),
      ['agent', 'repair', '--root', '/workspace', '--seam-id', '67fc764ba37d77bd', '--phase', 'before', '--json']
    );
  });

  test('findLiveRepairAttemptForSeam returns only awaiting or prepared rows', () => {
    const rows: AttemptInventoryRow[] = [
      { attemptId: 'a1', seamId: 'seam-a', state: 'awaiting_edit', disposition: 'current', command: null },
      { attemptId: 'a2', seamId: 'seam-a', state: 'finished_current', disposition: 'current', command: null },
      { attemptId: 'a3', seamId: 'seam-b', state: 'prepared', disposition: 'current', command: null },
    ];
    assert.strictEqual(findLiveRepairAttemptForSeam(rows, 'seam-a')?.attemptId, 'a1');
    assert.strictEqual(findLiveRepairAttemptForSeam(rows, 'seam-b')?.attemptId, 'a3');
    assert.strictEqual(findLiveRepairAttemptForSeam(rows, 'missing'), undefined);
  });

  test('parseBeforePhaseRepairStart reads attempt, edit cage, and related test', () => {
    const stdout = JSON.stringify({
      schema_version: '0.5',
      packets: [
        {
          seam_id: '67fc764ba37d77bd',
          allowed_edit_surface: ['tests/pricing.rs'],
          must_not_change: ['Do not edit production code.'],
          missing_discriminators: [{ value: 'discount_threshold (equality boundary)' }],
          nearest_strong_test_to_imitate: {
            file: 'tests/pricing.rs',
            line: 4,
            name: 'below_threshold_has_no_discount',
          },
        },
      ],
      repair_attempt: {
        attempt_id: 'repair-attempt-0123456789abcdef01234567',
        manifest_path: 'target/ripr/repair-attempts/repair-attempt-0123456789abcdef01234567/attempt.json',
        next_command: 'ripr agent repair --root . --attempt repair-attempt-0123456789abcdef01234567 --phase after',
      },
    });
    const parsed = parseBeforePhaseRepairStart(stdout);
    assert.ok(parsed);
    assert.strictEqual(parsed?.attemptId, 'repair-attempt-0123456789abcdef01234567');
    assert.strictEqual(parsed?.seamId, '67fc764ba37d77bd');
    assert.strictEqual(parsed?.relatedTestFile, 'tests/pricing.rs');
    assert.strictEqual(parsed?.relatedTestLine, 4);
    assert.ok(parsed?.summaryLines.some((line) => line.includes('not complete')));
    assert.ok(parsed?.summaryLines.some((line) => line.includes('tests/pricing.rs')));
  });

  test('extractAgentRepairLoopTarget finds the seam-bound repair handoff payload', () => {
    const target = extractAgentRepairLoopTarget([
      {
        title: 'Start repair: copy repair command',
        command: {
          command: 'ripr.copyAgentRepairCommand',
          title: 'Start repair: copy repair command',
          arguments: [
            {
              label: 'agent_repair',
              root: '.',
              seam_id: '67fc764ba37d77bd',
              target_artifact: 'target/ripr/repair-attempts',
              command: 'ripr agent repair --root . --seam-id 67fc764ba37d77bd --phase before',
            },
          ],
        },
      },
    ]);
    assert.ok(target);
    assert.strictEqual(seamIdFromAgentRepairTarget(target), '67fc764ba37d77bd');
  });
});

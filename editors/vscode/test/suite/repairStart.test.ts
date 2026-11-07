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

  test('findLiveRepairAttemptForSeam returns only resumable awaiting or prepared rows', () => {
    const rows: AttemptInventoryRow[] = [
      { attemptId: 'a1', seamId: 'seam-a', state: 'awaiting_edit', disposition: 'resumable', command: null },
      { attemptId: 'a2', seamId: 'seam-a', state: 'awaiting_edit', disposition: 'prepared_at_other_head', command: null },
      { attemptId: 'a3', seamId: 'seam-a', state: 'awaiting_edit', disposition: 'retry_budget_spent', command: null },
      { attemptId: 'a4', seamId: 'seam-a', state: 'finished_current', disposition: 'resumable', command: null },
      { attemptId: 'a5', seamId: 'seam-b', state: 'prepared', disposition: 'resumable', command: null },
    ];
    assert.strictEqual(findLiveRepairAttemptForSeam(rows, 'seam-a')?.attemptId, 'a1');
    assert.strictEqual(findLiveRepairAttemptForSeam(rows, 'seam-b')?.attemptId, 'a5');
    assert.strictEqual(findLiveRepairAttemptForSeam(rows, 'missing'), undefined);
  });

  test('Start does not reuse an ambiguous or unavailable retained baseline (#13216)', () => {
    const rows: AttemptInventoryRow[] = [
      { attemptId: 'ambiguous', seamId: 'seam-a', state: 'awaiting_edit', disposition: 'baseline_ambiguous', command: 'ripr agent repair --attempt ambiguous --phase after' },
      { attemptId: 'unavailable', seamId: 'seam-a', state: 'awaiting_edit', disposition: 'baseline_unavailable', command: 'ripr agent repair --attempt unavailable --phase after' }
    ];
    // The shared disposition decides even if a recorded after command is
    // present. Operational AwaitingEdit is not permission to continue.
    assert.strictEqual(findLiveRepairAttemptForSeam(rows, 'seam-a'), undefined);
    const clean: AttemptInventoryRow = { attemptId: 'clean', seamId: 'seam-a', state: 'awaiting_edit', disposition: 'resumable', command: 'ripr agent repair --attempt clean --phase after' };
    assert.strictEqual(findLiveRepairAttemptForSeam([...rows, clean], 'seam-a')?.attemptId, 'clean');
  });

  test('parseBeforePhaseRepairStart reads attempt, edit cage, and related test', () => {
    const parsed = parseBeforePhaseRepairStart(beforePhaseStdout());
    assert.ok(parsed);
    assert.strictEqual(parsed?.baselineAmbiguous, false);
    assert.strictEqual(parsed?.attemptId, 'repair-attempt-0123456789abcdef01234567');
    assert.strictEqual(parsed?.seamId, '67fc764ba37d77bd');
    assert.strictEqual(parsed?.relatedTestFile, 'tests/pricing.rs');
    assert.strictEqual(parsed?.relatedTestLine, 4);
    assert.ok(parsed?.summaryLines.some((line) => line.includes('not complete')));
    assert.ok(parsed?.summaryLines.some((line) => line.includes('tests/pricing.rs')));
    assert.ok(parsed?.summaryLines.some((line) => line.includes('Do not edit production code.')));
    assert.ok(parsed?.summaryLines.some((line) => line.includes('Editing outside the cage fails terminally.')));
  });

  test('an ambiguous before baseline preserves identity without suggesting the test edit (#13131)', () => {
    const parsed = parseBeforePhaseRepairStart(beforePhaseStdout(true));
    assert.ok(parsed);
    assert.strictEqual(parsed.baselineAmbiguous, true);
    assert.strictEqual(parsed.attemptId, 'repair-attempt-0123456789abcdef01234567');
    assert.strictEqual(
      parsed.manifestPath,
      'target/ripr/repair-attempts/repair-attempt-0123456789abcdef01234567/attempt.json'
    );
    // The producer retains this after command; it is metadata, not the restart route.
    assert.strictEqual(
      parsed.nextCommand,
      'ripr agent repair --root . --attempt repair-attempt-0123456789abcdef01234567 --phase after'
    );
    const summary = parsed.summaryLines.join('\n');
    assert.match(summary, /ambiguous/);
    assert.match(summary, /Incomparable/);
    assert.match(summary, /--phase before/);
    assert.match(summary, /clean state/);
    assert.ok(summary.includes(parsed.manifestPath!));
    assert.doesNotMatch(summary, /awaiting an external test edit|Next after the edit|Allowed edit surface|Related test:/);
    assert.ok(!summary.includes(parsed.nextCommand!));
  });

  test('an explicit comparable before baseline retains the ordinary edit guidance', () => {
    const parsed = parseBeforePhaseRepairStart(beforePhaseStdout(false));
    assert.ok(parsed);
    assert.strictEqual(parsed.baselineAmbiguous, false);
    assert.match(parsed.summaryLines.join('\n'), /awaiting an external test edit/);
    assert.match(parsed.summaryLines.join('\n'), /Next after the edit:/);
    assert.doesNotMatch(parsed.summaryLines.join('\n'), /ambiguous|Incomparable/);
  });

  test('a malformed baseline ambiguity flag is not treated as a comparable result', () => {
    for (const flag of ['true', null, 0, {}, []]) {
      assert.strictEqual(parseBeforePhaseRepairStart(beforePhaseStdout(flag)), undefined);
    }
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

// Matches render_before_phase_attempt_stdout: the ambiguity field is emitted
// only when true; the actual attempt identity and after command remain present.
function beforePhaseStdout(baselineAmbiguous?: unknown): string {
  return JSON.stringify({
    schema_version: '0.5',
    packets: [
      {
        seam_id: '67fc764ba37d77bd',
        allowed_edit_surface: ['tests/pricing.rs'],
        must_not_change: ['Do not edit production code.', 'Editing outside the cage fails terminally.'],
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
      packet_path: 'target/ripr/workflow/agent-packet.json',
      ...(baselineAmbiguous === undefined ? {} : { baseline_ambiguous: baselineAmbiguous }),
    },
  });
}

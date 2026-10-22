import * as assert from 'assert';
import * as path from 'path';
import { firstPrCommandIsSafe, validateFirstPrPacket } from '../../src/firstPrProjection';
import { hasUnsafeShellMetacharacter } from '../../src/packetJson';

suite('First-PR command safety', () => {
  const workspaceRoot = path.resolve('first-pr workspace');

  for (const [name, command] of [
    ['LF', 'ripr agent receipt --root . --json\nprintf REVIEW_EXTRA_COMMAND'],
    ['CRLF', 'ripr agent receipt --root . --json\r\nprintf REVIEW_EXTRA_COMMAND'],
    ['leading CR', '\rripr agent receipt --root . --json'],
    ['standalone CR', 'ripr agent receipt --root . --json\rprintf REVIEW_EXTRA_COMMAND'],
    ['quoted LF', "ripr agent receipt --root 'workspace\nname' --json"]
  ]) {
    for (const placement of ['commands', 'selected'] as const) {
      test(`rejects raw ${name} in ${placement} before normalization`, () => {
        assert.strictEqual(hasUnsafeShellMetacharacter(command), true);
        assert.strictEqual(packetStatus(command, workspaceRoot, placement).state, 'unsafeCommand', JSON.stringify(command));
      });
    }
  }

  test('rejects line breaks at command edges before trimming', () => {
    for (const command of [
      '\nripr agent receipt --root . --json',
      'ripr agent receipt --root . --json\n'
    ]) {
      assert.strictEqual(firstPrCommandIsSafe(command, [workspaceRoot]), false);
    }
  });

  test('retains admitted single-line command text and quoted argument bytes', () => {
    for (const command of [
      'ripr agent receipt --root . --json',
      ' \tripr\t agent  receipt --root . --json\t ',
      "ripr agent receipt --root 'repo  root' --json",
      "python -m pytest 'tests/test  pricing.py::test_discount'"
    ]) {
      const packet = packetStatus(command, workspaceRoot, 'selected');
      assert.strictEqual(packet.state, 'topRepairableGap', command);
      assert.strictEqual(packet.receiptCommand, command);
    }
  });

  test('preserves single-quoted redirect paths while normalizing inter-token whitespace', () => {
    const root = path.resolve('first-pr  workspace').replace(/\\/g, '/');
    // Exercise the currently admitted single-quote grammar on either host.
    // Windows double-quote admission remains a separate owner obligation.
    for (const command of [
      `ripr agent packet --root . --gap-id 'gap:amount>=threshold' --json > '${root}/target/ripr/out.json'`,
      ` \tripr  agent\tpacket --root . --gap-id gap:pricing --json \t>\t '${root}/target/ripr/out.json'\t `
    ]) {
      assert.strictEqual(firstPrCommandIsSafe(command, [root]), true, command);
    }
  });

  test('retains rejection of operators, malformed quotes and escaping redirects', () => {
    const outside = `${path.resolve('outside-first-pr').replace(/\\/g, '/')}/out.json`;
    for (const command of [
      'ripr agent receipt --root . --json\0',
      'ripr agent receipt --root . --json ; calc',
      'ripr agent receipt --root "$(calc)" --json',
      "ripr agent receipt --root 'unterminated --json",
      'ripr agent receipt --root "unterminated --json',
      `ripr agent packet --root . --gap-id gap:pricing --json > '${outside}'`,
      `ripr agent packet --root . --gap-id gap:pricing --json > target/ripr/ok.json > '${outside}'`,
      `ripr agent receipt --root "C:/work/ripr's repo" --json > '${outside}'`
    ]) {
      assert.strictEqual(firstPrCommandIsSafe(command, [workspaceRoot]), false, command);
    }
  });

  test('preserves quoted whitespace and redirect-like bytes as argument data', () => {
    const root = path.resolve('first-pr  workspace').replace(/\\/g, '/');
    const command = `ripr\tagent packet --root . --gap-id 'gap:a  >\tb' --json\t>\t'${root}/target/ripr/out.json'`;
    assert.strictEqual(firstPrCommandIsSafe(command, [root]), true, command);
  });
});

function packetStatus(command: string, workspaceRoot: string, placement: 'commands' | 'selected') {
  const packet = {
    schema_version: '0.1',
    tool: 'ripr',
    kind: 'first_pr_start_here',
    root: '.',
    status: 'actionable',
    posture: 'advisory',
    selected: {
      state: 'top_gap',
      gap_id: 'gap:pricing',
      verify_command: 'cargo xtask fixtures boundary_gap',
      receipt_command: placement === 'selected' ? command : 'ripr agent receipt --root . --json'
    },
    commands: placement === 'commands' ? { receipt: command } : {}
  };
  return validateFirstPrPacket(JSON.stringify(packet), workspaceRoot,
    'target/ripr/first-pr/start-here.json', 'target/ripr/first-pr/start-here.md',
    path.join(workspaceRoot, 'target/ripr/first-pr/start-here.json'),
    path.join(workspaceRoot, 'target/ripr/first-pr/start-here.md'));
}

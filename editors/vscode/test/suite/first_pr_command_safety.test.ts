import * as assert from 'assert';
import * as os from 'os';
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
    // Exercise the admitted single-quote grammar on either host; the Windows
    // console form is pinned below with the host stubbed (#9526).
    for (const command of [
      `ripr agent packet --root . --gap-id 'gap:amount>=threshold' --json > '${root}/target/ripr/out.json'`,
      ` \tripr  agent\tpacket --root . --gap-id gap:pricing --json \t>\t '${root}/target/ripr/out.json'\t `
    ]) {
      assert.strictEqual(firstPrCommandIsSafe(command, [root]), true, command);
    }
  });

  test('admits the server\'s Windows console form only on a Windows host (#9526)', () => {
    const root = path.resolve('first-pr  workspace').replace(/\\/g, '/');
    const spaced = `ripr agent receipt --root "${root}" --json`;
    const redirected = `ripr agent packet --root . --gap-id "gap:a  >b" --json > "${root}/target/ripr/out.json"`;
    // Off Windows a double quote is refused, as before.
    withHost('linux', () => {
      for (const command of [spaced, redirected]) {
        assert.strictEqual(firstPrCommandIsSafe(command, [root]), false, command);
      }
    });
    withHost('win32', () => {
      for (const command of [spaced, redirected]) {
        assert.strictEqual(firstPrCommandIsSafe(command, [root]), true, command);
        const packet = packetStatus(command, root, 'selected');
        assert.strictEqual(packet.state, 'topRepairableGap', command);
        assert.strictEqual(packet.receiptCommand, command);
      }
      for (const command of [
        // An odd backslash run escapes the closing quote for argv and Git Bash.
        'ripr agent receipt --root "a\\" --json',
        // cmd.exe expands %NAME% inside the quoted redirect target.
        'ripr agent packet --root . --gap-id gap:pricing --json > "%TEMP%/out.json"',
        // A bare `^` is cmd.exe's escape character.
        'ripr agent receipt --root HEAD^ --json'
      ]) {
        assert.strictEqual(firstPrCommandIsSafe(command, [root]), false, command);
        assert.strictEqual(packetStatus(command, root, 'selected').state, 'unsafeCommand', command);
      }
    });
  });

  test('retains rejection of operators, malformed quotes and escaping redirects', () => {
    const outside = `${path.resolve('outside-first-pr').replace(/\\/g, '/')}/out.json`;
    const apostropheRoot = path.join(os.tmpdir(), "ripr's repo").replace(/\\/g, '/');
    assert.ok(path.isAbsolute(apostropheRoot));
    assert.ok(apostropheRoot.includes("'") && apostropheRoot.includes(' '));
    for (const command of [
      'ripr agent receipt --root . --json\0',
      'ripr agent receipt --root . --json ; calc',
      'ripr agent receipt --root "$(calc)" --json',
      "ripr agent receipt --root 'unterminated --json",
      'ripr agent receipt --root "unterminated --json',
      `ripr agent packet --root . --gap-id gap:pricing --json > '${outside}'`,
      `ripr agent packet --root . --gap-id gap:pricing --json > target/ripr/ok.json > '${outside}'`,
      `ripr agent receipt --root "${apostropheRoot}" --json > '${outside}'`
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

/** Run `body` as if the extension host ran on `platform` (`isWindowsHost` reads `process.platform`). */
function withHost(platform: NodeJS.Platform, body: () => void): void {
  const original = Object.getOwnPropertyDescriptor(process, 'platform');
  Object.defineProperty(process, 'platform', { value: platform });
  try {
    body();
  } finally {
    if (original) {
      Object.defineProperty(process, 'platform', original);
    }
  }
}

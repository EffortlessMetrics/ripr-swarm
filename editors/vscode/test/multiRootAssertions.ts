import * as assert from 'assert';
import * as path from 'path';

// Literal fixture expectations, independent of the server's path/root helpers.
export interface RootExpectation {
  root: string;
  source: string;
  sourceContents: string;
  testContents: string;
  symbol: string;
  changedExpression: string;
  relatedTest: string;
  testNames: readonly string[];
}

interface CommandObservation { command: string; arguments?: unknown[] }
type ObjectValue = Record<string, any>;

function object(value: unknown): ObjectValue {
  assert.ok(value !== null && typeof value === 'object' && !Array.isArray(value));
  return value as ObjectValue;
}

// VS Code spells Windows drive URIs with a lowercased drive and an encoded
// colon (`file:///f%3A/dir`), while the LSP server re-serializes the same URI
// with an unencoded colon (`file:///f:/dir`). Both name one file, so URI
// comparisons normalize the drive spelling instead of pinning one serializer.
export function normalizeUriSpelling(value: string): string {
  return value.replace(/^file:\/\/\/([A-Za-z]):\//,
    (_match, drive: string) => `file:///${drive.toLowerCase()}%3A/`);
}

export function assertContextPacket(value: unknown, expected: RootExpectation): void {
  const envelope = object(value);
  assert.strictEqual(envelope.schema_version, '0.5');
  assert.strictEqual(envelope.packets_total, 1);
  assert.ok(Array.isArray(envelope.packets));
  assert.strictEqual(envelope.packets.length, 1, 'fixture must produce one actual seam packet');
  const packet = object(envelope.packets[0]);
  assert.strictEqual(packet.file, expected.source);
  assert.strictEqual(packet.owner, `${expected.source}::${expected.symbol}`);
  assert.strictEqual(packet.changed_expression, expected.changedExpression);
  assert.strictEqual(packet.line, 2);
  assert.strictEqual(packet.current_grip, 'weakly_gripped');
  assert.strictEqual(packet.task, 'write_targeted_test');
  assert.deepStrictEqual(packet.allowed_edit_surface, [expected.relatedTest]);
  assert.deepStrictEqual(packet.forbidden_files, [expected.source]);
  const recommended = object(packet.recommended_test);
  assert.strictEqual(recommended.file, expected.relatedTest);
  assert.strictEqual(recommended.name, `${expected.symbol}_boundary_discriminator`);
  assert.ok(Array.isArray(packet.related_existing_tests));
  assert.deepStrictEqual(packet.related_existing_tests.map((test: unknown) => {
    const entry = object(test);
    assert.strictEqual(entry.file, expected.relatedTest);
    assert.strictEqual(entry.relation_reason, 'direct_owner_call');
    assert.strictEqual(entry.oracle_kind, 'exact_value');
    return entry.name;
  }).sort(), [...expected.testNames].sort());
}

const artifactFields = {
  before_snapshot: 'target/ripr/pilot/repo-exposure.json',
  after_snapshot: 'target/ripr/pilot/after.repo-exposure.json',
  agent_packet_json: 'target/ripr/agent/agent-packet.json',
  agent_brief_json: 'target/ripr/agent/agent-brief.json',
  agent_verify_json: 'target/ripr/agent/agent-verify.json',
  agent_receipt_json: 'target/ripr/agent/agent-receipt.json',
};

const commandArtifacts: Record<string, string> = {
  'ripr.copyAgentRepairCommand': 'target/ripr/repair-attempts',
  'ripr.copyAgentPacketCommand': artifactFields.agent_packet_json,
  'ripr.copyAgentBriefCommand': artifactFields.agent_brief_json,
  'ripr.copyAfterSnapshotCommand': artifactFields.after_snapshot,
  'ripr.copyAgentVerifyCommand': artifactFields.agent_verify_json,
  'ripr.copyAgentReceiptCommand': artifactFields.agent_receipt_json,
};

// Inspect simple rendered command arguments; never execute/evaluate shell text.
function words(command: string): string[] {
  return (command.match(/'[^']*'|"[^"]*"|[^\s]+/g) ?? []).map(word =>
    /^(['"]).*\1$/.test(word) ? word.slice(1, -1) : word);
}

function normalized(file: string): string { return file.replace(/\\/g, '/'); }

function assertCommandRoot(command: string, expected: RootExpectation): string[] {
  const tokens = words(command);
  assert.strictEqual(tokens.filter(token => token === '--root').length, 1);
  assert.strictEqual(normalized(tokens[tokens.indexOf('--root') + 1]), normalized(expected.root));
  return tokens;
}

export function assertActionTargets(commands: CommandObservation[], expected: RootExpectation,
  sourceUri: string, relatedUri: string): void {
  for (const command of commands) {
    if (commandArtifacts[command.command]) {
      assert.strictEqual(command.arguments?.length, 1, 'rooted action needs its actual target');
    }
    for (const argument of command.arguments ?? []) {
      const target = object(argument);
      if (target.uri !== undefined) {
        assert.strictEqual(normalizeUriSpelling(target.uri as string),
          normalizeUriSpelling(command.command === 'ripr.openRelatedTest' ? relatedUri : sourceUri));
      }
      const artifact = commandArtifacts[command.command];
      if (artifact) {
        assert.strictEqual(target.root, '.'); // Metadata convention; command must carry the absolute root.
        assert.strictEqual(target.seam_file, expected.source);
        assert.strictEqual(target.owner, `${expected.source}::${expected.symbol}`);
        assert.strictEqual(target.line, 2);
        assert.strictEqual(target.seam_kind, 'predicate_boundary');
        assert.strictEqual(target.mode, 'draft');
        assert.strictEqual(target.base, 'HEAD~1');
        assert.strictEqual(target.target_artifact, artifact);
        for (const [field, value] of Object.entries(artifactFields)) { assert.strictEqual(target[field], value); }
        assert.strictEqual(typeof target.command, 'string');
        const tokens = assertCommandRoot(target.command, expected);
        if (['ripr.copyAgentPacketCommand', 'ripr.copyAgentBriefCommand',
          'ripr.copyAfterSnapshotCommand', 'ripr.copyAgentVerifyCommand'].includes(command.command)) {
          assert.strictEqual(tokens[tokens.length - 2], '>');
          assert.strictEqual(normalized(tokens[tokens.length - 1]), normalized(path.join(expected.root, artifact)));
        }
        if (command.command === 'ripr.copyAgentVerifyCommand') {
          assert.strictEqual(tokens[tokens.indexOf('--before') + 1], artifactFields.before_snapshot);
          assert.strictEqual(tokens[tokens.indexOf('--after') + 1], artifactFields.after_snapshot);
        }
        if (command.command === 'ripr.copyAgentReceiptCommand') {
          assert.strictEqual(tokens[tokens.indexOf('--verify-json') + 1], artifactFields.agent_verify_json);
          assert.strictEqual(tokens[tokens.indexOf('--out') + 1], artifact);
        }
      }
      if (target.label === 'repair_card') {
        const card = object(JSON.parse(target.packet));
        assert.strictEqual(card.schema_version, 'repair_card.v1');
        assert.strictEqual(card.changed_behavior, expected.changedExpression);
        assert.deepStrictEqual(card.allowed_files, [expected.relatedTest]);
        assert.strictEqual(object(card.selected_target).file, expected.relatedTest);
        assertCommandRoot(object(card.next_action).display, expected);
        assertCommandRoot(object(object(card.canonical_next_action).command).display, expected);
      }
      if (command.command === 'ripr.copyTargetedTestBrief') {
        assert.ok(target.brief.includes(`- ${expected.source}:2\n`));
        assert.ok(target.brief.includes(`- owner: ${expected.source}::${expected.symbol}\n`));
        assert.ok(target.brief.includes(`- Suggested file: ${expected.relatedTest}\n`));
      }
      if (command.command === 'ripr.copySuggestedAssertion') {
        assert.strictEqual(target.assertion,
          `assert_eq!(${expected.symbol}(/* boundary input where ${expected.changedExpression} */), /* expected */)`);
      }
    }
  }
  for (const name of Object.keys(commandArtifacts)) {
    assert.strictEqual(commands.filter(command => command.command === name).length, 1,
      `fixture must exercise ${name}, including its root and artifact containment`);
  }
}

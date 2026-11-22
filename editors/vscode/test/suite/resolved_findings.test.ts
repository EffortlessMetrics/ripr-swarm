import * as assert from 'assert';
import { resolvedFindingsMessage, resolvedFindingsNote } from '../../src/resolvedFindings';

suite('resolvedFindings (#13636)', () => {
  const row = 'src/pricing.rs:88: now exposed: a test appears to observe the changed behavior (`discount_boundary`)';

  test('reads the server rows and words one now-exposed changed behavior with its row', () => {
    const note = resolvedFindingsNote({ count: 1, rows: [{ text: row }], omitted: 0, evidence: 'static' });
    assert.deepStrictEqual(note, { count: 1, lines: [row] });
    assert.strictEqual(
      resolvedFindingsMessage(note!),
      `ripr: 1 changed behavior is now exposed since the last analysis: a test appears to observe it (static evidence). ${row}`
    );
  });

  test('counts every gap and names the rows the server left out', () => {
    const note = resolvedFindingsNote({ count: 3, rows: [{ text: row }], omitted: 2 });
    assert.deepStrictEqual(note, { count: 3, lines: [row, '+2 more'] });
    assert.strictEqual(
      resolvedFindingsMessage(note!),
      'ripr: 3 changed behaviors are now exposed since the last analysis: tests appear to observe them (static evidence).'
    );
  });

  test('stays silent when nothing moved or the payload is absent or malformed', () => {
    for (const value of [undefined, null, 'x', {}, { count: 0, rows: [] }, { count: -1 }, { count: 1.5, rows: [{ text: row }] }]) {
      assert.strictEqual(resolvedFindingsNote(value), undefined, JSON.stringify(value));
    }
  });

  test('never claims a runtime mutation outcome', () => {
    const message = resolvedFindingsMessage({ count: 2, lines: [row] });
    for (const word of ['killed', 'caught', 'survived', 'covered', 'proven']) { // ripr-allow: static-language: negative oracle listing banned words
      assert.ok(!message.toLowerCase().includes(word), `${word} in ${message}`);
    }
  });
});

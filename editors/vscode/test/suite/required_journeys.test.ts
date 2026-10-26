import * as assert from 'assert';
import * as fs from 'fs';
import * as path from 'path';
import {
  ALL_REQUIRED_JOURNEYS,
  REQUIRED_TITLE_SEPARATOR,
  fullTitle,
  requiredJourneyTitles,
} from '../requiredJourneys';

// Compiled to out/test/suite; the sources the journeys name live in test/suite.
const suiteSources = path.resolve(__dirname, '../../../test/suite');

suite('required journeys', () => {
  test('every required journey is registered under its suite in the suite sources', () => {
    for (const journey of ALL_REQUIRED_JOURNEYS) {
      const source = fs.readFileSync(path.join(suiteSources, journey.file), 'utf8');
      assert.ok(source.includes(`suite('${journey.suite}'`), `${journey.file} has no suite '${journey.suite}'`);
      assert.ok(source.includes(`test('${journey.test}'`), `${journey.file} has no test '${journey.test}'`);
    }
  });

  test('no required title contains the env separator', () => {
    for (const journey of ALL_REQUIRED_JOURNEYS) {
      assert.ok(!fullTitle(journey).includes(REQUIRED_TITLE_SEPARATOR), fullTitle(journey));
    }
  });

  test('an untrusted host requires only the untrusted trust-boundary test', () => {
    assert.deepStrictEqual(requiredJourneyTitles('untrusted', '/srv/ripr', '/srv/ripr'), [
      'Packaged Workspace Trust untrusted host keeps an actionable repair packet out of the clipboard',
    ]);
  });

  test('a trusted host without a server path does not require real-server journeys', () => {
    assert.deepStrictEqual(requiredJourneyTitles('trusted', undefined, undefined), [
      'Packaged Workspace Trust trusted host copies an actionable repair packet',
    ]);
  });

  test('a trusted host with a server path requires the real-server journeys', () => {
    const titles = requiredJourneyTitles('trusted', '/srv/ripr', undefined);
    assert.strictEqual(titles.length, 3);
    assert.ok(titles.some((title) => title.includes('real server surfaces seam diagnostic')));
    assert.ok(titles.some((title) => title.includes('real server invalidates a seam action')));
  });

  test('a packaged server path adds the packaged handshake', () => {
    const titles = requiredJourneyTitles('trusted', '/srv/ripr', '/srv/ripr');
    assert.ok(titles.some((title) => title.includes('accepts the real packaged server')));
  });
});

import * as assert from 'assert';
import {
  statusBarColors,
  statusText,
  type RiprStatusKind
} from '../../src/statusRender';

suite('Status bar render kinds (#8587)', () => {
  test('resolution failure keeps the server-missing label', () => {
    const text = statusText('serverUnavailable');
    assert.strictEqual(text, '$(warning) ripr: server missing');
    assert.ok(
      !text.includes('server failed'),
      'resolution failure must not reuse the start-failure label'
    );
  });

  test('a resolved server that failed to start is not labeled missing', () => {
    const startFailureKind: RiprStatusKind = 'serverFailed';
    const text = statusText(startFailureKind);
    assert.strictEqual(text, '$(warning) ripr: server failed');
    assert.ok(
      !text.includes('server missing'),
      'start failure must not point the user at a missing-binary fix'
    );
    const missing = statusText('serverUnavailable');
    assert.notStrictEqual(
      text,
      missing,
      'start-failure and resolution-failure labels must stay distinct'
    );
    assert.ok(
      statusBarColors(startFailureKind),
      'start failure must keep an error status-bar color rather than idle default'
    );
    assert.ok(
      statusBarColors('serverUnavailable'),
      'resolution failure remains an error color'
    );
    assert.strictEqual(
      statusBarColors('ready'),
      undefined,
      'ready stays the idle default so error coloring is not universal'
    );
  });
});

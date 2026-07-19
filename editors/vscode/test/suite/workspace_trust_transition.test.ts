import * as assert from 'assert';
import { RiprClientController } from '../../src/client';
import { startAfterWorkspaceTrust } from '../../src/extension';

suite('Workspace Trust Transition', () => {
  test('concurrent trust-grant starts coalesce into one controller start', async () => {
    let releaseStart: (() => void) | undefined;
    const startGate = new Promise<void>((resolve) => {
      releaseStart = resolve;
    });
    let startCalls = 0;
    const controller = {
      start: async () => {
        startCalls += 1;
        await startGate;
      }
    } as Pick<RiprClientController, 'start'>;

    const first = startAfterWorkspaceTrust(controller);
    const second = startAfterWorkspaceTrust(controller);
    await Promise.resolve();

    assert.strictEqual(startCalls, 1);
    releaseStart?.();
    await Promise.all([first, second]);
  });

  test('a failed trust-grant start can be retried', async () => {
    let startCalls = 0;
    const controller = {
      start: async () => {
        startCalls += 1;
        if (startCalls === 1) {
          throw new Error('sentinel start failure');
        }
      }
    } as Pick<RiprClientController, 'start'>;

    await assert.rejects(
      startAfterWorkspaceTrust(controller),
      /sentinel start failure/
    );
    await startAfterWorkspaceTrust(controller);

    assert.strictEqual(startCalls, 2);
  });
});

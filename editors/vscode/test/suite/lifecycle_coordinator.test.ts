import * as assert from 'assert';
import { RiprClientController } from '../../src/client';
import {
  restartServerOnce,
  startServerOnce,
  stopServerOnce
} from '../../src/extension';

interface Deferred {
  promise: Promise<void>;
  resolve: () => void;
  reject: (error: Error) => void;
}

function deferred(): Deferred {
  let resolve: (() => void) | undefined;
  let reject: ((error: Error) => void) | undefined;
  const promise = new Promise<void>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return {
    promise,
    resolve: () => resolve?.(),
    reject: (error: Error) => reject?.(error)
  };
}

type LifecycleController = Pick<RiprClientController, 'start' | 'stop'>;

suite('Extension Lifecycle Coordinator', () => {
  test('restart waits for in-flight startup before stopping or creating a replacement', async () => {
    const firstStart = deferred();
    let startCalls = 0;
    let stopCalls = 0;
    const controller = {
      start: async () => {
        startCalls += 1;
        if (startCalls === 1) {
          await firstStart.promise;
        }
      },
      stop: async () => {
        stopCalls += 1;
      }
    } as LifecycleController;

    const initialStart = startServerOnce(controller);
    const restart = restartServerOnce(controller, 1_000);
    await Promise.resolve();

    assert.strictEqual(stopCalls, 0, 'a Starting client must not receive stop()');
    assert.strictEqual(startCalls, 1, 'no replacement starts while the first start is unresolved');

    firstStart.resolve();
    await Promise.all([initialStart, restart]);

    assert.strictEqual(stopCalls, 1);
    assert.strictEqual(startCalls, 2, 'restart creates exactly one replacement after startup settles');
  });

  test('concurrent restart requests coalesce into one stop and replacement start', async () => {
    const replacementStart = deferred();
    let startCalls = 0;
    let stopCalls = 0;
    const controller = {
      start: async () => {
        startCalls += 1;
        if (startCalls === 2) {
          await replacementStart.promise;
        }
      },
      stop: async () => {
        stopCalls += 1;
      }
    } as LifecycleController;

    await startServerOnce(controller);
    const firstRestart = restartServerOnce(controller, 1_000);
    const secondRestart = restartServerOnce(controller, 1_000);
    await Promise.resolve();
    await Promise.resolve();

    assert.strictEqual(stopCalls, 1);
    assert.strictEqual(startCalls, 2);

    replacementStart.resolve();
    await Promise.all([firstRestart, secondRestart]);

    assert.strictEqual(stopCalls, 1, 'coalesced restart must not stop twice');
    assert.strictEqual(startCalls, 2, 'coalesced restart must not create two replacements');
  });

  test('shutdown waits for in-flight startup before stopping', async () => {
    const firstStart = deferred();
    let stopCalls = 0;
    const controller = {
      start: async () => firstStart.promise,
      stop: async () => {
        stopCalls += 1;
      }
    } as LifecycleController;

    const initialStart = startServerOnce(controller);
    const stop = stopServerOnce(controller, 1_000);
    await Promise.resolve();

    assert.strictEqual(stopCalls, 0);
    firstStart.resolve();
    await Promise.all([initialStart, stop]);
    assert.strictEqual(stopCalls, 1);
  });

  test('startup rejection still permits one bounded recovery restart', async () => {
    const firstStart = deferred();
    let startCalls = 0;
    let stopCalls = 0;
    const controller = {
      start: async () => {
        startCalls += 1;
        if (startCalls === 1) {
          await firstStart.promise;
        }
      },
      stop: async () => {
        stopCalls += 1;
      }
    } as LifecycleController;

    const initialStart = startServerOnce(controller);
    const observedInitial = initialStart.then(
      () => undefined,
      (error: unknown) => error
    );
    const restart = restartServerOnce(controller, 1_000);
    firstStart.reject(new Error('sentinel startup rejection'));

    const initialError = await observedInitial;
    assert.match(String(initialError), /sentinel startup rejection/);
    await restart;

    assert.strictEqual(stopCalls, 1);
    assert.strictEqual(startCalls, 2, 'recovery starts one fresh session after rejection');
  });

  test('wedged startup reaches a finite failure without stopping or replacing the client', async () => {
    const firstStart = deferred();
    let startCalls = 0;
    let stopCalls = 0;
    const controller = {
      start: async () => {
        startCalls += 1;
        await firstStart.promise;
      },
      stop: async () => {
        stopCalls += 1;
      }
    } as LifecycleController;

    const initialStart = startServerOnce(controller);
    await assert.rejects(
      restartServerOnce(controller, 5),
      /startup did not settle within 5ms/
    );

    assert.strictEqual(stopCalls, 0, 'timeout must not call stop() on a Starting client');
    assert.strictEqual(startCalls, 1, 'timeout must not start a replacement client');

    firstStart.resolve();
    await initialStart;
  });
});

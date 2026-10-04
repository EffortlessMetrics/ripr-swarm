import * as assert from 'assert';
import * as fs from 'fs';
import * as path from 'path';

const ALLOWED_COMPLETION_EVENT_PREFIXES = ['onCommand:', 'onSettingChanged:', 'onContext:', 'onView:', 'onLink:', 'extensionInstalled:'];

// VS Code built-in when-clause keys this walkthrough may name after `onContext:`.
// `workspaceTrusted` is not a VS Code context key; the built-in is `isWorkspaceTrusted`.
const DOCUMENTED_ON_CONTEXT_KEYS = new Set(['isWorkspaceTrusted', 'resourceLangId']);

interface WalkthroughStep {
  id: string;
  title: string;
  media?: { markdown?: string };
  completionEvents?: string[];
}

const EXPECTED_FLOW: Array<{ id: string; media: string; completionEvents: string[] }> = [
  {
    id: 'ripr.trustWorkspace',
    media: 'walkthrough/trust.md',
    completionEvents: ['onContext:isWorkspaceTrusted']
  },
  {
    id: 'ripr.openRustFile',
    media: 'walkthrough/open-file.md',
    completionEvents: ['onContext:resourceLangId == rust']
  },
  {
    id: 'ripr.readDiagnostics',
    media: 'walkthrough/diagnostics.md',
    completionEvents: []
  },
  {
    id: 'ripr.tryCodeAction',
    media: 'walkthrough/code-action.md',
    completionEvents: ['onCommand:ripr.showStatus']
  }
];

function onContextKey(event: string): string | undefined {
  if (!event.startsWith('onContext:')) {
    return undefined;
  }
  const expression = event.slice('onContext:'.length).trim();
  const match = /^[A-Za-z_][A-Za-z0-9_]*/.exec(expression);
  return match?.[0];
}

suite('Walkthrough Contribution Contract', () => {
  // Compiled tests live under out/test/suite; walk three levels back
  // to the extension root rather than resolving package/media under out/.
  const packageJsonPath = path.resolve(__dirname, '../../../package.json');
  const extensionRoot = path.resolve(__dirname, '../../..');

  function manifest(): { id: string; steps: WalkthroughStep[] } {
    const parsed = JSON.parse(fs.readFileSync(packageJsonPath, 'utf8'));
    const walkthroughs = parsed.contributes?.walkthroughs;
    assert.ok(Array.isArray(walkthroughs) && walkthroughs.length === 1, 'expected exactly one walkthrough');
    return walkthroughs[0];
  }

  test('the get-started walkthrough pins its ordered flow, media, and completion events', () => {
    const walkthrough = manifest();
    assert.strictEqual(walkthrough.id, 'ripr.getStarted');
    const actual = walkthrough.steps.map((step) => ({
      id: step.id,
      media: step.media?.markdown,
      completionEvents: step.completionEvents ?? []
    }));
    assert.deepStrictEqual(actual, EXPECTED_FLOW);
  });

  test('bare onContext completion events name documented VS Code context keys', () => {
    assert.ok(
      !DOCUMENTED_ON_CONTEXT_KEYS.has('workspaceTrusted'),
      'workspaceTrusted must not be treated as a documented VS Code context key'
    );
    assert.strictEqual(onContextKey('onContext:workspaceTrusted'), 'workspaceTrusted');
    assert.strictEqual(onContextKey('onContext:isWorkspaceTrusted'), 'isWorkspaceTrusted');
    assert.strictEqual(onContextKey('onContext:resourceLangId == rust'), 'resourceLangId');
    assert.strictEqual(onContextKey('onCommand:ripr.showStatus'), undefined);
  });

  test('every media file exists and completion events use the supported vocabulary', () => {
    for (const step of EXPECTED_FLOW) {
      assert.ok(
        fs.existsSync(path.join(extensionRoot, step.media)),
        `walkthrough media does not exist: ${step.media}`
      );
      for (const event of step.completionEvents) {
        assert.ok(
          ALLOWED_COMPLETION_EVENT_PREFIXES.some((prefix) => event.startsWith(prefix)),
          `unsupported completion event on step ${step.id}: ${event}`
        );
        const contextKey = onContextKey(event);
        if (contextKey !== undefined) {
          assert.ok(
            DOCUMENTED_ON_CONTEXT_KEYS.has(contextKey),
            `onContext event on step ${step.id} names undocumented key ${contextKey}: ${event}`
          );
        }
      }
    }
  });
});

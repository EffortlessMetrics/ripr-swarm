import * as assert from 'assert';
import * as fs from 'fs';
import * as path from 'path';

const ALLOWED_COMPLETION_EVENT_PREFIXES = ['onCommand:', 'onContext:', 'onView:', 'onLink:', 'extensionInstalled:'];

interface WalkthroughStep {
  id: string;
  title: string;
  media?: { markdown?: string };
  completionEvents?: string[];
}

suite('Walkthrough Contribution Contract', () => {
  const packageJsonPath = path.resolve(__dirname, '../../package.json');
  const extensionRoot = path.resolve(__dirname, '../..');

  function steps(): WalkthroughStep[] {
    const manifest = JSON.parse(fs.readFileSync(packageJsonPath, 'utf8'));
    const walkthroughs = manifest.contributes?.walkthroughs;
    assert.ok(Array.isArray(walkthroughs) && walkthroughs.length === 1, 'expected exactly one walkthrough');
    return walkthroughs[0].steps as WalkthroughStep[];
  }

  test('the get-started walkthrough has four unique steps with existing media files', () => {
    const seen = new Set<string>();
    const all = steps();
    assert.strictEqual(all.length, 4, 'walkthrough step count drifted');
    for (const step of all) {
      assert.ok(step.id && !seen.has(step.id), `duplicate or missing step id: ${step.id}`);
      seen.add(step.id);
      const media = step.media?.markdown;
      assert.ok(media, `step ${step.id} is missing markdown media`);
      assert.ok(
        fs.existsSync(path.join(extensionRoot, media)),
        `step ${step.id} media does not exist: ${media}`
      );
    }
  });

  test('completion events use the supported walkthrough vocabulary', () => {
    for (const step of steps()) {
      for (const event of step.completionEvents ?? []) {
        assert.ok(
          ALLOWED_COMPLETION_EVENT_PREFIXES.some((prefix) => event.startsWith(prefix)),
          `unsupported completion event on step ${step.id}: ${event}`
        );
      }
    }
  });
});

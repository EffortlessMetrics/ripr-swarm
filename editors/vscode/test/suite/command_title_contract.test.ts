import * as assert from 'assert';
import * as fs from 'fs';
import * as path from 'path';

// The palette title of a command is what a status message or a doc tells the
// user to type. These tests bind those mentions to the manifest, so renaming a
// command cannot leave the product pointing at a title nobody can find (#15094).

interface ContributedCommand {
  command: string;
  title: string;
}

const extensionRoot = path.resolve(__dirname, '..', '..', '..');
const repoRoot = path.resolve(extensionRoot, '..', '..');

function contributedCommands(): ContributedCommand[] {
  const manifest = JSON.parse(fs.readFileSync(path.join(extensionRoot, 'package.json'), 'utf8'));
  return manifest.contributes.commands as ContributedCommand[];
}

function titleOf(command: string): string {
  const found = contributedCommands().find((entry) => entry.command === command);
  assert.ok(found, `${command} must be a contributed command`);
  return found.title;
}

function read(...segments: string[]): string {
  return fs.readFileSync(path.join(...segments), 'utf8');
}

suite('Command title contract', () => {
  test('in-product text names the refresh command by its palette title (#15094)', () => {
    const refreshTitle = titleOf('ripr.refreshDiagnostics');
    const client = read(extensionRoot, 'src', 'client.ts');
    const constant = /const REFRESH_DIAGNOSTICS_COMMAND_TITLE = '([^']+)';/.exec(client);
    assert.ok(constant, 'client.ts must define REFRESH_DIAGNOSTICS_COMMAND_TITLE');
    assert.strictEqual(constant[1], refreshTitle);

    // Every "ripr: Refresh ..." mention in source, settings text and the
    // extension README must be the contributed title, not an older one.
    const surfaces: Array<[string, string]> = [
      ['src/client.ts', client],
      ['src/statusRender.ts', read(extensionRoot, 'src', 'statusRender.ts')],
      ['package.json', read(extensionRoot, 'package.json')],
      ['README.md', read(extensionRoot, 'README.md')],
      ['docs/EDITOR_EXTENSION.md', read(repoRoot, 'docs', 'EDITOR_EXTENSION.md')]
    ];
    let mentions = 0;
    for (const [name, text] of surfaces) {
      for (const match of text.matchAll(/ripr: Refresh(?: [A-Z][a-z]+)+/g)) {
        mentions += 1;
        assert.strictEqual(match[0], refreshTitle, `${name} names a refresh command that is not in the palette`);
      }
    }
    assert.ok(mentions >= 5, `expected the refresh title to be mentioned across the surfaces, saw ${mentions}`);
  });

  test('the Commands list in docs/EDITOR_EXTENSION.md is the contributed titles, in both directions (#15094)', () => {
    const doc = read(repoRoot, 'docs', 'EDITOR_EXTENSION.md');
    const start = doc.indexOf('\n## Commands\n');
    assert.ok(start >= 0, 'docs/EDITOR_EXTENSION.md must have a Commands section');
    const rest = doc.slice(start + '\n## Commands\n'.length);
    const nextHeading = rest.search(/\n#{2,3} /);
    const section = nextHeading >= 0 ? rest.slice(0, nextHeading) : rest;
    const listed = [...section.matchAll(/^- `(ripr: [^`]+)`$/gm)].map((match) => match[1]);
    const contributed = contributedCommands().map((entry) => entry.title);
    assert.ok(listed.length > 0, 'the Commands section must list command titles');
    assert.deepStrictEqual([...listed].sort(), [...contributed].sort());
  });

  test('the command table in docs/CONFIGURATION.md matches the manifest ids and titles (#15094)', () => {
    const doc = read(repoRoot, 'docs', 'CONFIGURATION.md');
    const rows = [...doc.matchAll(/^\| `(ripr\.[A-Za-z]+)` \| (ripr: [^|]+?) \|$/gm)].map(
      (match) => `${match[1]} = ${match[2]}`
    );
    const contributed = contributedCommands().map((entry) => `${entry.command} = ${entry.title}`);
    assert.deepStrictEqual([...rows].sort(), [...contributed].sort());
  });

  test('every command the extension README lists is a contributed title (#15094)', () => {
    const readme = read(extensionRoot, 'README.md');
    const contributed = new Set(contributedCommands().map((entry) => entry.title));
    const listed = [...readme.matchAll(/^- `(ripr: [^`]+)`$/gm)].map((match) => match[1]);
    assert.ok(listed.length > 0, 'the README command reference must list command titles');
    for (const title of listed) {
      assert.ok(contributed.has(title), `README lists ${title}, which the extension does not contribute`);
    }
  });
});

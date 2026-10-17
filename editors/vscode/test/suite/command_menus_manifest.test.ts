//
// Command menus / keybindings manifest contract (#2081).
//
// The repair-loop commands were palette-only: no `contributes.menus`, no
// `contributes.keybindings`, no grouping. This suite pins the contributed
// editor/context menu entries (the repair-loop group, gated on the ripr
// document languages), the two default keybindings, and the wiring
// invariant that every menu/keybinding command id is also a contributed
// command (fail-closed against a dangling menu entry).
//
// The targeted-test and agent-loop commands stay code-action-only: their
// handlers need the diagnostic payload a code action carries, which a
// context-menu invocation cannot supply (the agent-loop commands already
// follow that rule, #2083). This suite also pins that exclusion.
//
// Platform note: VS Code when-clauses have no diagnostic-source context
// key, so gating on `diagnosticSource == ripr` is not implementable; the
// menu entries gate on `editorTextFocus` + `resourceLangId` instead. The
// language set mirrors RIPR_DOCUMENT_SELECTORS in src/client.ts.
//

import * as assert from 'assert';
import { promises as fs } from 'fs';
import * as path from 'path';

interface MenuEntry {
  command?: string;
  submenu?: string;
  when?: string;
  group?: string;
}

interface KeybindingEntry {
  command?: string;
  key?: string;
  mac?: string;
  when?: string;
}

interface CommandMenusManifest {
  contributes?: {
    commands?: Array<{ command?: string; title?: string }>;
    submenus?: Array<{ id?: string; label?: string }>;
    menus?: {
      'editor/context'?: MenuEntry[];
      'ripr.editorContext'?: MenuEntry[];
    };
    keybindings?: KeybindingEntry[];
  };
}

const RIPR_MENU_WHEN =
  'editorTextFocus && resourceLangId =~ /^(rust|typescript|typescriptreact|javascript|javascriptreact|python)$/';

// #7678: the editor context menu contributed eight contiguous ripr entries,
// three of them near-identical clipboard commands, filling a large share of
// the most contested menu surface. The top-packet copy/inspect cockpit now
// lives in the ripr.editorContext submenu; only the highest-frequency action
// (Copy Repair Packet at Cursor, already keybound) stays at the top level.
const EXPECTED_EDITOR_CONTEXT: Array<{ submenu: string; group: string }> = [
  { submenu: 'ripr.editorContext', group: 'ripr@2' }
];

const EXPECTED_TOP_LEVEL_COMMANDS: Array<{ command: string; group: string }> = [
  { command: 'ripr.copyRepairPacketAtCursor', group: 'ripr@1' }
];

const EXPECTED_EDITOR_CONTEXT_SUBMENU: Array<{ command: string; group: string }> = [
  { command: 'ripr.copyTopRepairPacket', group: 'ripr@1' },
  { command: 'ripr.copyTopVerifyCommand', group: 'ripr@2' },
  { command: 'ripr.copyTopReceiptCommand', group: 'ripr@3' },
  { command: 'ripr.showTopLimitation', group: 'ripr@4' },
  { command: 'ripr.showStatus', group: 'ripr@5' },
  { command: 'ripr.showReceiptStatus', group: 'ripr@6' },
  { command: 'ripr.showRouteQuality', group: 'ripr@7' }
];

// Payload-bound commands that must stay code-action-only: the targeted-test
// group and the agent-loop group (#2083) both need the diagnostic payload a
// code action carries, which an editor/context menu click cannot supply.
const CODE_ACTION_ONLY_COMMANDS = [
  'ripr.copySuggestedAssertion',
  'ripr.copyTargetedTestBrief',
  'ripr.openRelatedTest',
  'ripr.copyAgentRepairCommand',
  'ripr.copyAgentPacketCommand',
  'ripr.copyAgentBriefCommand',
  'ripr.copyAfterSnapshotCommand',
  'ripr.copyAgentVerifyCommand',
  'ripr.copyAgentReceiptCommand'
];

const EXPECTED_KEYBINDINGS: Array<{ command: string; key: string; mac: string }> = [
  { command: 'ripr.copyRepairPacketAtCursor', key: 'ctrl+alt+shift+p', mac: 'cmd+alt+shift+p' },
  { command: 'ripr.showStatus', key: 'ctrl+alt+r', mac: 'cmd+alt+r' },
  { command: 'ripr.copyTopRepairPacket', key: 'ctrl+alt+p', mac: 'cmd+alt+p' }
];

async function readManifest(): Promise<CommandMenusManifest> {
  const manifestPath = path.resolve(__dirname, '../../../package.json');
  return JSON.parse(await fs.readFile(manifestPath, 'utf8')) as CommandMenusManifest;
}

suite('Command Menus Manifest', () => {
  test('receipt copy command titles disclose their source and match the README', async () => {
    const manifest = await readManifest();
    const commands = manifest.contributes?.commands ?? [];
    const expectedTitles = [
      { command: 'ripr.copyReceiptCommand', title: 'ripr: Copy Receipt Command (Receipt Status)' },
      { command: 'ripr.copyTopReceiptCommand', title: 'ripr: Copy Receipt Command (Top Repair Packet)' },
      { command: 'ripr.copyFirstPrReceiptCommand', title: 'ripr: First PR - Copy Receipt Command' },
      { command: 'ripr.copyAgentReceiptCommand', title: 'ripr: Review Result - Copy Receipt Command' }
    ];
    const readme = await fs.readFile(path.resolve(__dirname, '../../../README.md'), 'utf8');
    const actualTitles = [];
    for (const expected of expectedTitles) {
      const matchingCommands = commands.filter((entry) => entry.command === expected.command);
      assert.strictEqual(matchingCommands.length, 1, `${expected.command} must be contributed exactly once`);
      assert.strictEqual(matchingCommands[0].title, expected.title, `${expected.command} must disclose its source`);
      actualTitles.push(matchingCommands[0].title);
      assert.ok(readme.includes(`- \`${expected.title}\``), `${expected.command} README title must match the palette`);
    }
    assert.strictEqual(new Set(actualTitles).size, expectedTitles.length, 'receipt copy commands must remain distinguishable');
  });

  test('receipt-status command title matches the current public command inventories', async () => {
    const manifest = await readManifest();
    const command = (manifest.contributes?.commands ?? [])
      .find((entry) => entry.command === 'ripr.copyReceiptCommand');
    assert.ok(command?.title, 'receipt-status copy command must have a contributed title');
    const expectedRows = [
      { file: 'CONFIGURATION.md', row: `| \`ripr.copyReceiptCommand\` | ${command.title} |` },
      { file: 'EDITOR_EXTENSION.md', row: `- \`${command.title}\`` },
      { file: 'LSP_AGENT_REPAIR_WORKFLOW.md', row: `| \`${command.title}\` | \`ripr.copyReceiptCommand\` |` }
    ];
    for (const expected of expectedRows) {
      const document = await fs.readFile(path.resolve(__dirname, '../../../../../docs', expected.file), 'utf8');
      assert.ok(document.includes(expected.row), `${expected.file} receipt-status inventory must match the palette`);
    }
  });

  test('editor/context keeps one top-level action and routes the cockpit through the ripr submenu', async () => {
    const manifest = await readManifest();
    const entries = manifest.contributes?.menus?.['editor/context'] ?? [];
    const topLevelCommands = entries
      .filter((entry) => entry.command)
      .map((entry) => ({ command: entry.command as string, group: entry.group as string }));
    const submenus = entries
      .filter((entry) => entry.submenu)
      .map((entry) => ({ submenu: entry.submenu as string, group: entry.group as string }));
    assert.deepStrictEqual(
      topLevelCommands,
      EXPECTED_TOP_LEVEL_COMMANDS,
      'editor/context must keep only the highest-frequency copy action at the top level (#7678)'
    );
    assert.deepStrictEqual(
      submenus,
      EXPECTED_EDITOR_CONTEXT,
      'editor/context must route the remaining repair-loop commands through the ripr submenu'
    );
    const contributedSubmenus = new Set(
      (manifest.contributes?.submenus ?? []).map((entry) => entry.id)
    );
    for (const entry of submenus) {
      assert.ok(contributedSubmenus.has(entry.submenu), `${entry.submenu} must be a contributed submenu`);
    }
    for (const entry of entries) {
      assert.strictEqual(
        entry.when,
        RIPR_MENU_WHEN,
        `${entry.command ?? entry.submenu} must gate on editorTextFocus and the ripr document languages`
      );
    }
  });

  test('ripr submenu carries the top-packet cockpit commands in stable order', async () => {
    const manifest = await readManifest();
    const entries = manifest.contributes?.menus?.['ripr.editorContext'] ?? [];
    assert.deepStrictEqual(
      entries.map((entry) => ({ command: entry.command, group: entry.group })),
      EXPECTED_EDITOR_CONTEXT_SUBMENU,
      'ripr submenu must hold the top-packet copy and inspection commands (#7678)'
    );
    for (const entry of entries) {
      assert.strictEqual(
        entry.when,
        RIPR_MENU_WHEN,
        `${entry.command} must keep the ripr language guard inside the submenu`
      );
    }
  });

  test('payload-bound targeted-test and agent-loop commands stay out of editor/context and the submenu', async () => {
    const manifest = await readManifest();
    const entries = new Set([
      ...(manifest.contributes?.menus?.['editor/context'] ?? []).map((entry) => entry.command),
      ...(manifest.contributes?.menus?.['ripr.editorContext'] ?? []).map((entry) => entry.command)
    ]);
    for (const command of CODE_ACTION_ONLY_COMMANDS) {
      assert.ok(
        !entries.has(command),
        `${command} needs a diagnostic payload and must stay code-action-only, not in editor/context`
      );
    }
  });

  test('default keybindings cover showStatus and copyTopRepairPacket and stay user-overridable', async () => {
    const manifest = await readManifest();
    const keybindings = manifest.contributes?.keybindings ?? [];
    assert.deepStrictEqual(
      keybindings.map((entry) => ({ command: entry.command, key: entry.key, mac: entry.mac })),
      EXPECTED_KEYBINDINGS,
      'expected default keybindings for the two highest-signal commands'
    );
    for (const entry of keybindings) {
      // The cursor-scoped copy command inspects the active selection, so its
      // keybinding must carry the same ripr-language guard as its
      // editor/context entry; the workspace-scoped commands intentionally
      // stay language-agnostic (#7344).
      const expectedWhen = entry.command === 'ripr.copyRepairPacketAtCursor'
        ? RIPR_MENU_WHEN
        : 'editorTextFocus';
      assert.strictEqual(entry.when, expectedWhen, `${entry.command} keybinding when-clause`);
    }
  });

  test('every menu and keybinding command id is a contributed command', async () => {
    const manifest = await readManifest();
    const contributed = new Set(
      (manifest.contributes?.commands ?? []).map((entry) => entry.command)
    );
    const referenced = [
      ...(manifest.contributes?.menus?.['editor/context'] ?? []).filter((entry) => !entry.submenu).map((entry) => entry.command),
      ...(manifest.contributes?.menus?.['ripr.editorContext'] ?? []).map((entry) => entry.command),
      ...(manifest.contributes?.keybindings ?? []).map((entry) => entry.command)
    ];
    for (const command of referenced) {
      assert.ok(command && contributed.has(command), `menu/keybinding references unregistered command ${command}`);
    }
    // Submenu entries carry no command; they must point at a contributed submenu instead.
    const contributedSubmenus = new Set((manifest.contributes?.submenus ?? []).map((entry) => entry.id));
    for (const entry of manifest.contributes?.menus?.['editor/context'] ?? []) {
      if (entry.submenu) {
        assert.ok(contributedSubmenus.has(entry.submenu), `editor/context references unregistered submenu ${entry.submenu}`);
      }
    }
  });
});

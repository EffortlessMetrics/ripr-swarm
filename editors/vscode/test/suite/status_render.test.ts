import * as assert from 'assert';
import * as vscode from 'vscode';
import { RiprClientController, RiprClientRuntime } from '../../src/client';
import { RiprConfig } from '../../src/config';
import {
  shouldShowMainStatusBar,
  statusBarColors,
  statusText,
  workspaceStatusDeliveryBudgetLines,
  type RiprStatusKind
} from '../../src/statusRender';
import { compatibleLspEvidence } from './testCompatibility';

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

suite('Passive stale status presentation (#5319)', () => {
  test('stale uses a neutral icon and label without warning styling', () => {
    const text = statusText('stale');
    assert.strictEqual(text, '$(circle-outline) ripr: unsaved changes');
    assert.ok(!text.includes('$(warning)'), text);
    assert.strictEqual(statusBarColors('stale'), undefined);
  });

  test('degraded and failure kinds keep warning or error styling', () => {
    assert.ok(statusBarColors('analysisLimited'));
    assert.ok(statusText('analysisLimited').includes('$(warning)'));

    assert.ok(statusBarColors('gapArtifactWarning'));
    assert.ok(statusText('gapArtifactWarning').includes('$(warning)'));

    assert.ok(statusBarColors('analysisFailed'));
    assert.ok(statusText('analysisFailed').includes('$(error)'));
  });

  test('first useful action stale branch keeps warning presentation', () => {
    const text = statusText('analysisReady', {
      status: 'stale',
      actionKind: 'write_focused_test',
      title: 'Add test',
      reportPath: '/tmp/report.json',
      warningCount: 0
    });
    assert.strictEqual(text, '$(warning) ripr: first action');
  });
});

const MAIN_STATUS_BAR_VISIBILITY: Record<RiprStatusKind, boolean> = {
  disabled: false,
  noWorkspace: true,
  workspaceUntrusted: true,
  workspaceAmbiguous: true,
  resolvingServer: true,
  serverUnavailable: false,
  serverFailed: true,
  circuitOpen: true,
  starting: true,
  analysisQueued: true,
  ready: true,
  analysisRunning: true,
  analysisReady: true,
  analysisLimited: true,
  languageNotEnabled: true,
  gapActionable: true,
  gapNoAction: true,
  gapArtifactWarning: true,
  noActionableSeams: true,
  noEnabledLanguages: true,
  stale: true,
  analysisFailed: true,
  stopped: true
};

suite('Main status-bar visibility (#5319)', () => {
  test('every status kind has an explicit visibility', () => {
    for (const [kind, visible] of Object.entries(MAIN_STATUS_BAR_VISIBILITY) as Array<[RiprStatusKind, boolean]>) {
      assert.strictEqual(
        shouldShowMainStatusBar(kind),
        visible,
        `${kind} should ${visible ? 'show' : 'hide'} the main status-bar item`
      );
    }
    const hidden = Object.entries(MAIN_STATUS_BAR_VISIBILITY)
      .filter(([, visible]) => !visible)
      .map(([kind]) => kind)
      .sort();
    assert.deepStrictEqual(
      hidden,
      ['disabled', 'serverUnavailable'],
      'only disabled and serverUnavailable hide the main item; noWorkspace stays visible'
    );
  });
});

const enabledConfig: RiprConfig = {
  enabled: true,
  serverPath: '/sentinel/trusted/ripr',
  serverArgs: ['lsp', '--stdio'],
  autoDownload: false,
  serverVersion: '',
  downloadBaseUrl: 'https://sentinel.invalid/ripr',
  checkMode: 'draft',
  baseRef: 'origin/main',
  includeUnchangedTests: true,
  seamDiagnostics: true,
  diagnosticProfile: 'actionable',
  traceServer: 'off'
};

suite('Show Status command (#5319)', () => {
  test('opens the output panel once and raises no notification', async () => {
    const infoMessages: string[] = [];
    const warningMessages: string[] = [];
    const errorMessages: string[] = [];
    let outputShown = 0;
    const outputLines: string[] = [];
    const runtime: RiprClientRuntime = {
      getConfig: () => enabledConfig,
      workspaceRootState: () => ({ kind: 'noWorkspace', roots: [] }),
      workspaceFolders: () => [],
      showQuickPick: async () => undefined,
      resolveServer: async () => ({
        command: '/sentinel/trusted/ripr',
        source: 'configured',
        detail: 'sentinel server',
        installationState: 'unmanaged',
        compatibilityResult: compatibleLspEvidence
      }),
      createLanguageClient: () => ({
        onNotification: () => ({ dispose: () => undefined }),
        sendRequest: async () => {
          throw new Error('Show Status must not contact the server in this test');
        },
        setTrace: () => undefined,
        start: async () => undefined,
        stop: async () => undefined
      }),
      createFileSystemWatcher: () => ({} as unknown as vscode.FileSystemWatcher),
      readFile: async () => undefined,
      runRipr: async () => '',
      writeClipboard: async () => undefined,
      isWorkspaceTrusted: () => true,
      showInformationMessage: async (message) => {
        infoMessages.push(message);
        return undefined;
      },
      showWarningMessage: async (message) => {
        warningMessages.push(message);
        return undefined;
      },
      showErrorMessage: async (message) => {
        errorMessages.push(message);
        return undefined;
      }
    };
    const output = {
      appendLine: (line: string) => {
        outputLines.push(line);
      },
      show: () => {
        outputShown += 1;
      }
    } as unknown as vscode.LogOutputChannel;
    const controller = new RiprClientController(
      {} as unknown as vscode.ExtensionContext,
      output,
      runtime
    );

    await controller.showStatus();

    assert.strictEqual(outputShown, 1, 'Show Status must reveal the output channel once');
    assert.deepStrictEqual(infoMessages, [], 'Show Status must not raise an information notification');
    assert.deepStrictEqual(warningMessages, [], 'Show Status must not raise a warning notification');
    assert.deepStrictEqual(errorMessages, [], 'Show Status must not raise an error notification');
    assert.ok(
      outputLines.some((line) => line.includes('ripr status:')),
      `output channel must receive the status summary: ${outputLines.join('\n')}`
    );
  });

  test('prints delivery-budget overflow lines from collectWorkspaceStatus', async () => {
    const outputLines: string[] = [];
    const runtime: RiprClientRuntime = {
      getConfig: () => enabledConfig,
      workspaceRootState: () => ({ kind: 'singleRoot', root: '/workspace', roots: ['/workspace'] }),
      workspaceFolders: () => [{ uri: vscode.Uri.file('/workspace'), name: 'workspace', index: 0 }],
      showQuickPick: async () => undefined,
      resolveServer: async () => ({
        command: '/sentinel/trusted/ripr',
        source: 'configured',
        detail: 'sentinel server',
        installationState: 'unmanaged',
        compatibilityResult: compatibleLspEvidence
      }),
      createLanguageClient: () => ({
        onNotification: () => ({ dispose: () => undefined }),
        sendRequest: async (method: string) => {
          if (method === 'workspace/executeCommand') {
            return {
              run_status: 'ready',
              diagnostic_budget: {
                overflowed: true,
                selected_count: 50,
                omitted_count: 1,
                eligible_items: 51,
                omitted: [{ reason: 'document_item_limit' }],
                continuation_or_inspect_route: 'ripr/listActionableItems',
              },
            };
          }
          throw new Error(`unexpected request ${method}`);
        },
        setTrace: () => undefined,
        start: async () => undefined,
        stop: async () => undefined
      }),
      createFileSystemWatcher: () => ({} as unknown as vscode.FileSystemWatcher),
      readFile: async () => undefined,
      runRipr: async () => '',
      writeClipboard: async () => undefined,
      isWorkspaceTrusted: () => true,
      showInformationMessage: async () => undefined,
      showWarningMessage: async () => undefined,
      showErrorMessage: async () => undefined
    };
    const output = {
      appendLine: (line: string) => {
        outputLines.push(line);
      },
      show: () => undefined
    } as unknown as vscode.LogOutputChannel;
    const controller = new RiprClientController(
      {} as unknown as vscode.ExtensionContext,
      output,
      runtime
    );
    await controller.start();
    await controller.showStatus();
    assert.ok(
      outputLines.some((line) => line.includes('50 selected, 1 omitted')),
      outputLines.join('\n')
    );
    await controller.stop();
  });
});

suite('Workspace status delivery budget disclosure (#2596)', () => {
  test('overflowed budget status names omitted counts and the retrieval route', () => {
    const lines = workspaceStatusDeliveryBudgetLines({
      diagnostic_budget: {
        overflowed: true,
        selected_count: 50,
        omitted_count: 1,
        eligible_items: 51,
        continuation_or_inspect_route: 'ripr/listActionableItems'
      }
    });
    assert.strictEqual(lines.length, 2);
    assert.ok(lines[0].includes('50 selected, 1 omitted'));
    assert.ok(lines[0].includes('bounded subset'));
    assert.ok(lines[1].includes('ripr/listActionableItems'));
  });

  test('profile-only omitted entries do not inflate delivery omission counts', () => {
    const lines = workspaceStatusDeliveryBudgetLines({
      diagnostic_budget: {
        overflowed: true,
        selected_count: 50,
        omitted_count: 3,
        eligible_items: 51,
        omitted: [
          { reason: 'profile_filtered' },
          { reason: 'profile_filtered' },
          { reason: 'document_item_limit' },
        ],
        continuation_or_inspect_route: 'ripr/listActionableItems',
      },
    });
    assert.ok(lines[0].includes('50 selected, 1 omitted'));
  });

  test('byte-cap overflow with zero omissions still warns', () => {
    const lines = workspaceStatusDeliveryBudgetLines({
      diagnostic_budget: {
        overflowed: true,
        selected_count: 1,
        omitted_count: 0,
        overflow_reasons: ['serialized_byte_limit'],
        continuation_or_inspect_route: 'ripr/listActionableItems',
      },
    });
    assert.strictEqual(lines.length, 2);
    assert.ok(lines[0].includes('byte cap'));
  });

  test('profile-only or complete budgets stay silent', () => {
    assert.deepStrictEqual(
      workspaceStatusDeliveryBudgetLines({
        diagnostic_budget: {
          overflowed: false,
          omitted_count: 3,
          selected_count: 0
        }
      }),
      []
    );
    assert.deepStrictEqual(workspaceStatusDeliveryBudgetLines({}), []);
  });
});

import * as vscode from 'vscode';
import {
  RiprClientController,
  RiprAgentLoopCommandTarget,
  RiprContextTarget,
  RiprRelatedTestTarget,
  RiprSuggestedAssertionTarget,
  RiprTargetedTestBriefTarget,
  RIPR_DOCUMENT_SELECTORS,
} from './client';
import {
  DEFAULT_LIFECYCLE_SETTLE_BUDGET_MS,
  ExtensionLifecycleCoordinator,
} from './lifecycleCoordinator';

let controller: RiprClientController | undefined;
let lifecycleCoordinator = new ExtensionLifecycleCoordinator();

/** Test-only reset for suites that exercise activation-scoped helpers with
 * independent fake controllers in one extension-host process. Production
 * activation resets the coordinator before constructing its controller. */
export function resetLifecycleCoordinatorForTests(): void {
  lifecycleCoordinator = new ExtensionLifecycleCoordinator();
}

export async function startServerOnce(
  currentController: Pick<RiprClientController, 'start'> | undefined
): Promise<void> {
  await lifecycleCoordinator.start(currentController);
}

export async function restartServerOnce(
  currentController: Pick<RiprClientController, 'start' | 'stop'> | undefined,
  startSettleBudgetMs = DEFAULT_LIFECYCLE_SETTLE_BUDGET_MS
): Promise<void> {
  await lifecycleCoordinator.restart(currentController, startSettleBudgetMs);
}

export async function stopServerOnce(
  currentController: Pick<RiprClientController, 'start' | 'stop'> | undefined,
  startSettleBudgetMs = DEFAULT_LIFECYCLE_SETTLE_BUDGET_MS
): Promise<void> {
  await lifecycleCoordinator.stop(currentController, startSettleBudgetMs);
}

export async function startAfterWorkspaceTrust(
  currentController: Pick<RiprClientController, 'start'> | undefined
): Promise<void> {
  await startServerOnce(currentController);
}

export async function startAfterFirstWorkspaceFolder(
  currentController: RiprClientController | undefined
): Promise<void> {
  currentController?.allowWorkspaceRootRecovery();
  await startServerOnce(currentController);
}

/** Shared entry point for the active-editor listener and controller tests. */
export async function synchronizeWorkspaceRootOnce(
  currentController: RiprClientController | undefined
): Promise<void> {
  if (!currentController?.prepareWorkspaceRootChange()) {
    return;
  }
  do {
    await restartServerOnce(currentController);
  } while (currentController.workspaceRootNeedsRestart());
}

/** Shared entry point for the bound-folder removal listener. */
export async function pauseRemovedWorkspaceRootOnce(
  currentController: RiprClientController | undefined,
  removed: readonly vscode.WorkspaceFolder[]
): Promise<void> {
  if (currentController?.sessionRootRemoved(removed)) {
    currentController.quarantineRemovedWorkspaceRoot();
    await lifecycleCoordinator.pause(currentController);
    currentController.completeWorkspaceRootRemoval();
  }
}

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  const output = vscode.window.createOutputChannel('ripr', { log: true });
  const status = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 100);
  // Second, independent item for the shared repair-attempt status (#4643,
  // RIPR-SPEC-0220). It renders only the CLI's typed attempt state and is
  // hidden whenever the server session stops; the durable per-root selection
  // lives in workspaceState, not in this item.
  const attemptStatus = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 99);
  lifecycleCoordinator = new ExtensionLifecycleCoordinator();
  controller = new RiprClientController(context, output, undefined, status, attemptStatus);

  context.subscriptions.push(
    output,
    status,
    attemptStatus,
    vscode.commands.registerCommand('ripr.restartServer', async () => {
      try {
        controller?.allowWorkspaceRootRecovery();
        await restartServerOnce(controller);
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        output.appendLine(`ripr server restart failed: ${message}`);
        // Restart is the recovery action every other status message names, so
        // a failed restart must offer the same one-click recovery the
        // missing-server and warning surfaces already carry (Show Output /
        // Retry), not a dead-end toast that sends the user back to the
        // palette (#7342).
        const selection = await vscode.window.showErrorMessage(
          `ripr could not restart the server: ${message}`,
          'Show Output',
          'Retry'
        );
        if (selection === 'Show Output') {
          controller?.showOutput();
        } else if (selection === 'Retry') {
          await vscode.commands.executeCommand('ripr.restartServer');
        }
      }
    }),
    vscode.commands.registerCommand('ripr.refreshDiagnostics', async () =>
      controller?.refreshDiagnostics()
    ),
    // No matching `onCommand:ripr.selectWorkspaceRoot` entry in package.json
    // `activationEvents`: since VS Code 1.74 a contributed command activates
    // its extension automatically, and engines.vscode is ^1.90.0. The
    // manifest's legacy onCommand list predates that guarantee and is not
    // extended for new commands. (#2077)
    vscode.commands.registerCommand('ripr.selectWorkspaceRoot', async () =>
      controller?.selectWorkspaceRoot()
    ),
    vscode.commands.registerCommand('ripr.showOutput', () => controller?.showOutput()),
    vscode.commands.registerCommand('ripr.showStatus', () => controller?.showStatus()),
    vscode.commands.registerCommand('ripr.showAttemptStatus', async (attemptId?: unknown) =>
      // VS Code may pass a non-string first argument (menu, keybinding, URI):
      // never forward it into the trim() guard inside showAttemptStatus.
      controller?.showAttemptStatus(typeof attemptId === 'string' ? attemptId : undefined)
    ),
    vscode.commands.registerCommand('ripr.diagnoseSetup', () => controller?.diagnoseSetup()),
    vscode.commands.registerCommand('ripr.startCurrentRepair', async () =>
      controller?.startCurrentRepair()
    ),
    vscode.commands.registerCommand('ripr.copyCurrentRepairPacket', async () =>
      controller?.copyCurrentRepairPacket()
    ),
    vscode.commands.registerCommand('ripr.copyRepoGapMap', async () =>
      controller?.copyRepoGapMap()
    ),
    vscode.commands.registerCommand('ripr.openFirstPrPacket', async () =>
      controller?.openFirstPrPacket()
    ),
    vscode.commands.registerCommand('ripr.copyFirstPrSummary', async () =>
      controller?.copyFirstPrSummary()
    ),
    vscode.commands.registerCommand('ripr.copyFirstPrRepairPacket', async () =>
      controller?.copyFirstPrRepairPacket()
    ),
    vscode.commands.registerCommand('ripr.copyRepairPacketAtCursor', async () =>
      controller?.copyRepairPacketAtCursor()
    ),
    vscode.commands.registerCommand('ripr.copyFirstPrVerifyCommand', async () =>
      controller?.copyFirstPrVerifyCommand()
    ),
    vscode.commands.registerCommand('ripr.copyFirstPrReceiptCommand', async () =>
      controller?.copyFirstPrReceiptCommand()
    ),
    vscode.commands.registerCommand('ripr.copyFirstPrRegenerationGuidance', async () =>
      controller?.copyFirstPrRegenerationGuidance()
    ),
    vscode.commands.registerCommand('ripr.copyContext', async (target?: RiprContextTarget) =>
      controller?.copyContext(target)
    ),
    vscode.commands.registerCommand(
      'ripr.copySuggestedAssertion',
      async (target?: RiprSuggestedAssertionTarget) => controller?.copySuggestedAssertion(target)
    ),
    vscode.commands.registerCommand(
      'ripr.copyTargetedTestBrief',
      async (target?: RiprTargetedTestBriefTarget) => controller?.copyTargetedTestBrief(target)
    ),
    vscode.commands.registerCommand(
      'ripr.copyAgentRepairCommand',
      async (target?: RiprAgentLoopCommandTarget) => controller?.copyAgentLoopCommand(target)
    ),
    vscode.commands.registerCommand(
      'ripr.copyAgentPacketCommand',
      async (target?: RiprAgentLoopCommandTarget) => controller?.copyAgentLoopCommand(target)
    ),
    vscode.commands.registerCommand(
      'ripr.copyAgentBriefCommand',
      async (target?: RiprAgentLoopCommandTarget) => controller?.copyAgentLoopCommand(target)
    ),
    vscode.commands.registerCommand(
      'ripr.copyAfterSnapshotCommand',
      async (target?: RiprAgentLoopCommandTarget) => controller?.copyAgentLoopCommand(target)
    ),
    vscode.commands.registerCommand(
      'ripr.copyAgentVerifyCommand',
      async (target?: RiprAgentLoopCommandTarget) => controller?.copyAgentLoopCommand(target)
    ),
    vscode.commands.registerCommand(
      'ripr.copyAgentReceiptCommand',
      async (target?: RiprAgentLoopCommandTarget) => controller?.copyAgentLoopCommand(target)
    ),
    vscode.commands.registerCommand('ripr.openRelatedTest', async (target?: RiprRelatedTestTarget) =>
      controller?.openRelatedTest(target)
    ),
    vscode.commands.registerCommand('ripr.openSettings', async () => {
      await vscode.commands.executeCommand('workbench.action.openSettings', 'ripr');
    }),
    // Cockpit commands
    vscode.commands.registerCommand('ripr.copyTopRepairPacket', async () =>
      controller?.copyTopRepairPacket()
    ),
    vscode.commands.registerCommand('ripr.copyTopVerifyCommand', async () =>
      controller?.copyTopVerifyCommand()
    ),
    vscode.commands.registerCommand('ripr.copyTopReceiptCommand', async () =>
      controller?.copyTopReceiptCommand()
    ),
    vscode.commands.registerCommand('ripr.openReport', async () =>
      controller?.openReport()
    ),
    vscode.commands.registerCommand('ripr.showTopLimitation', async () =>
      controller?.showTopLimitation()
    ),
    // Receipt status / route-quality inspection commands
    vscode.commands.registerCommand('ripr.showReceiptStatus', async () =>
      controller?.showReceiptStatus()
    ),
    vscode.commands.registerCommand('ripr.copyReceiptCommand', async () =>
      controller?.copyReceiptCommand()
    ),
    vscode.commands.registerCommand('ripr.openAttemptLedger', async () =>
      controller?.openAttemptLedger()
    ),
    vscode.commands.registerCommand('ripr.showRouteQuality', async () =>
      controller?.showRouteQuality()
    ),
    vscode.workspace.onDidChangeTextDocument((event) => {
      if (event.document.isDirty) {
        controller?.markWorkspaceStale(event.document);
      }
    }),
    vscode.workspace.onDidSaveTextDocument((document) => {
      controller?.markWorkspaceSaved(document);
    }),
    vscode.workspace.onDidCloseTextDocument((document) => {
      controller?.markWorkspaceClosed(document);
    }),
    vscode.workspace.onDidChangeWorkspaceFolders(async (event) => {
      if (controller?.sessionRootRemoved(event.removed)) {
        output.appendLine('ripr selected workspace folder was removed; pausing until explicit root recovery.');
        try {
          await pauseRemovedWorkspaceRootOnce(controller, event.removed);
        } catch (error) {
          output.appendLine(`ripr server pause after workspace folder removal failed: ${String(error)}`);
        }
        return;
      }
      if (event.added.length === 0) {
        return;
      }
      output.appendLine(
        `ripr workspace folder added (${event.added.length}); starting the server if none is running.`
      );
      try {
        await startServerOnce(controller);
      } catch (error) {
        output.appendLine(`ripr server start after workspace folder change failed: ${String(error)}`);
      }
    }),
    vscode.workspace.onDidGrantWorkspaceTrust(async () => {
      output.appendLine('ripr workspace trust granted; starting a fresh server session.');
      try {
        await startAfterWorkspaceTrust(controller);
      } catch (error) {
        output.appendLine(`ripr server start after workspace trust failed: ${String(error)}`);
      }
    }),
    // Auto-start when a workspace folder appears after a no-workspace start.
    // Without this listener, opening a single Rust file first (no folder)
    // hits the noWorkspace early return in client.ts and stops. Opening
    // a folder later does nothing — the user has to run `ripr: Restart
    // Server` manually. We only react when the PRE-event folder count was
    // zero and at least one folder was added, so we don't restart on
    // routine folder additions/removals in an already-running multi-root
    // workspace. The pre-event count is derived from the post-event count
    // by adding back removals and subtracting additions. (#2015)
    vscode.workspace.onDidChangeWorkspaceFolders(async (event) => {
      const folderCount = vscode.workspace.workspaceFolders?.length ?? 0;
      // Reconstruct the pre-event folder count. VS Code does not expose it
      // directly; derive it from the post-event count and the event delta.
      const preEventFolderCount = folderCount + event.removed.length - event.added.length;
      const zeroToFolderTransition = preEventFolderCount === 0 && event.added.length > 0;
      if (zeroToFolderTransition && !controller?.isRunning()) {
        output.appendLine('ripr workspace folder added; starting server session.');
        try {
          await startAfterFirstWorkspaceFolder(controller);
        } catch (error) {
          output.appendLine(`ripr server start after workspace folder added failed: ${String(error)}`);
        }
      }
    }),
    // A multi-root window with no active file starts in workspaceAmbiguous.
    // Opening a file from one folder selects that folder (#2077).
    vscode.window.onDidChangeActiveTextEditor(async (editor) => {
      if (
        !editor
        || !RIPR_DOCUMENT_SELECTORS.some(selector =>
          selector.scheme === editor.document.uri.scheme && selector.language === editor.document.languageId
        )
        || !vscode.workspace.getWorkspaceFolder(editor.document.uri)
      ) {
        return;
      }
      try {
        await synchronizeWorkspaceRootOnce(controller);
      } catch (error) {
        output.appendLine(`ripr server start after selecting a workspace file failed: ${String(error)}`);
      }
    }),
    vscode.workspace.onDidChangeConfiguration(async (event) => {
      if (
        event.affectsConfiguration('ripr.enabled') ||
        event.affectsConfiguration('ripr.server') ||
        event.affectsConfiguration('ripr.check') ||
        event.affectsConfiguration('ripr.baseRef')
      ) {
        try {
          await restartServerOnce(controller);
        } catch (error) {
          const message = error instanceof Error ? error.message : String(error);
          output.appendLine(`ripr server restart after configuration change failed: ${message}`);
        }
        return;
      }
      if (event.affectsConfiguration('ripr.trace')) {
        // Trace applies live (#2082): a full restart would drop every
        // published diagnostic and force a full re-analysis. When a
        // restart-triggering key changed in the same event, the restart
        // above already applied the new trace level at start().
        controller?.setTraceFromConfig();
      }
    })
  );

  try {
    await startServerOnce(controller);
  } catch (error) {
    // A handshake failure is already surfaced by the controller (status bar
    // and a warning), and rejecting activation would only add a generic
    // "Activating extension failed" toast. Other start failures are logged
    // here and stay visible in the output channel.
    output.appendLine(`ripr server start during activation failed: ${String(error)}`);
  }
}

export async function deactivate(): Promise<void> {
  const currentController = controller;
  controller = undefined;
  await stopServerOnce(currentController);
}

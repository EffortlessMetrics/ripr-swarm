/**
 * Clipboard and agent-handoff commands extracted from client.ts (#2541).
 */
import { promises as fs } from 'fs';
import * as path from 'path';
import * as vscode from 'vscode';
import {
  cockpitUnavailableMessage,
  interpretCockpitObjectResponse,
  type CockpitRequestAbsence,
  type CockpitRequestResult
} from './cockpitRequest';
import {
  hasUnsafeShellMetacharacter,
  redirectTargetMatches,
  serverShellArg,
  isWindowsHost
} from './packetJson';
import type { RiprConfig } from './config';
import type { ResolvedServer } from './serverResolver';
import type { RiprFirstPrPacketStatus } from './firstPrProjection';
import type {
  RiprClientRuntime,
  RiprContextTarget,
  RiprSuggestedAssertionTarget,
  RiprTargetedTestBriefTarget,
  RiprAgentLoopCommandTarget,
  RiprActionableGapQueueStatus,
  RiprActionableGapQueueState,
  RiprLanguageClient,
  RiprSetupStatus,
  RiprStatusContext,
  RiprReceiptArtifactStatus
} from './client';

export interface AgentHandoffHost {
  readonly runtime: RiprClientRuntime;
  readonly output: vscode.LogOutputChannel;
  getClient(): RiprLanguageClient | undefined;
  getServer(): ResolvedServer | undefined;
  getWorkspaceRoot(): string | undefined;
  getStartGeneration(): number;
  getStatusKind(): string;
  getSetupStatus(): RiprSetupStatus;
  statusContext(): RiprStatusContext;
  refreshSetupStatusFiles(): Promise<void>;
  resolveServerForCommand(config: RiprConfig): Promise<ResolvedServer | undefined>;
  copyActionRootBlocker(targetUri?: vscode.Uri): string | undefined;
  activeDocumentRootBlocker(document: vscode.TextDocument): string | undefined;
  actionRootStillCurrent(
    generation: number,
    uri?: vscode.Uri,
    rootBlocker?: (targetUri?: vscode.Uri) => string | undefined
  ): boolean;
  workspaceFolderForUri(uri: vscode.Uri): vscode.WorkspaceFolder | undefined;
  warnWithOutput(message: string): void;
  setupRepairBlocker(): string | undefined;
  isRiprFileDocument(document: vscode.TextDocument): boolean;
}

const DIAGNOSTIC_SCOPED_COMMAND_GUIDANCE =
  'ripr targeted-test commands run from a ripr diagnostic: open a file with ripr diagnostics, use Quick Fix (Ctrl+. or Cmd+.) on one, and pick the targeted-test command from there.';

export { DIAGNOSTIC_SCOPED_COMMAND_GUIDANCE };

interface StartRepairAction {
  title: string;
  command: vscode.Command;
  priority: number;
}

function actionableGapQueueAllowsCurrentRepairPacket(queue: RiprActionableGapQueueStatus): boolean {
  return queue.state === 'topActionableGap'
    && Boolean(queue.canonicalGapId)
    && Boolean(queue.topRepair)
    && Boolean(queue.relatedTest)
    && Boolean(queue.verifyCommand)
    && Boolean(queue.receiptCommandOrPath)
    && queue.languageStatus !== 'disabled'
    && queue.languageStatus !== 'unavailable'
    && queue.languageStatus !== 'unsupported';
}

function actionableGapQueueCanBecomeStale(state: RiprActionableGapQueueState): boolean {
  return state === 'topActionableGap'
    || state === 'noAction'
    || state === 'reportOnly'
    || state === 'staticLimitOnly'
    || state === 'blocked';
}

function actionableGapQueueSuppressedMessage(queue: RiprActionableGapQueueStatus): string {
  switch (queue.state) {
    case 'missing':
      return 'ripr actionable gap queue is not generated for this workspace; use the ripr diagnostic hover and code actions for the current finding.';
    case 'malformed':
      return 'ripr actionable gap queue is malformed; repair packet actions are suppressed.';
    case 'unsupportedSchema':
      return 'ripr actionable gap queue schema is unsupported; repair packet actions are suppressed.';
    case 'wrongRoot':
      return 'ripr actionable gap queue belongs to another workspace; repair packet actions are suppressed.';
    case 'stale':
      return 'ripr actionable gap queue is stale; refresh saved-workspace evidence before copying repair packets.';
    case 'unsafePath':
      return 'ripr actionable gap queue contains an unsafe path; repair packet actions are suppressed.';
    case 'unsafeCommand':
      return 'ripr actionable gap queue contains an unsafe command; repair packet actions are suppressed.';
    case 'blocked':
      return 'ripr actionable gap queue is blocked by missing typed fields or producer limitations; repair packet actions are suppressed.';
    case 'reportOnly':
      return 'ripr actionable gap queue is report-only; no current repair packet is available.';
    case 'staticLimitOnly':
      return 'ripr actionable gap queue is static-limit-only; no current repair packet is available.';
    case 'noAction':
      return 'ripr actionable gap queue has no actionable gap; no current repair packet is available.';
    case 'noWorkspace':
      return 'Open a workspace before copying a ripr current repair packet.';
    case 'topActionableGap':
      return 'ripr actionable gap queue is missing required repair packet fields; repair packet actions are suppressed.';
  }
}

function currentRepairPacket(queue: RiprActionableGapQueueStatus): string {
  const lines = [
    'RIPR current repair packet',
    '',
    'Task',
    `Repair this one canonical gap: ${queue.canonicalGapId ?? 'unknown'}.`,
    '',
    'Context'
  ];
  pushOptionalLine(lines, 'Source', queue.sourceFile);
  pushOptionalLine(lines, 'Language', queue.languageStatus ? `${queue.language ?? 'unknown'} (${queue.languageStatus})` : queue.language);
  pushOptionalLine(lines, 'Evidence class', queue.evidenceClass);
  pushOptionalLine(lines, 'Actionability', queue.actionability);
  pushOptionalLine(lines, 'Changed behavior', queue.changedBehavior);
  pushOptionalLine(lines, 'Why this matters', queue.why);
  pushOptionalLine(lines, 'Missing discriminator', queue.missingDiscriminator);
  pushOptionalLine(lines, 'Focused proof intent', queue.focusedProofIntent);
  pushOptionalLine(lines, 'Confidence basis', queue.confidenceBasis);
  lines.push(`Related test: ${queue.relatedTest ?? 'not provided by typed queue artifact'}`);
  lines.push('');
  lines.push('Repair');
  pushOptionalLine(lines, 'Repair kind', queue.topRepair);
  pushOptionalLine(lines, 'Target test type', queue.targetTestType);
  pushOptionalLine(lines, 'Target assertion or output proof', queue.assertionShape);
  lines.push('');
  lines.push('Verification');
  lines.push(`Run: ${queue.verifyCommand}`);
  lines.push('');
  lines.push('Receipt');
  lines.push(`Record: ${queue.receiptCommandOrPath}`);
  lines.push('');
  lines.push('Stop conditions');
  lines.push('- Stop if the queue artifact is stale, wrong-root, malformed, or missing this canonical gap id.');
  lines.push('- Stop if the related test or target assertion no longer matches the changed behavior.');
  lines.push('- Stop if the verify command cannot be run safely in the current workspace.');
  lines.push('');
  lines.push('Do not do');
  lines.push('- Do not broaden scope beyond this one gap.');
  lines.push('- Do not edit production code unless the focused test exposes a real issue.');
  lines.push('- Do not generate tests automatically, call providers, run mutation execution, or claim runtime adequacy.');
  lines.push('- Do not treat this advisory static packet as a gate decision, merge approval, or policy eligibility claim.');
  return lines.join('\n');
}

function actionableGapQueueAllowsRepoGapMap(queue: RiprActionableGapQueueStatus): boolean {
  return queue.state === 'topActionableGap'
    || queue.state === 'noAction'
    || queue.state === 'reportOnly'
    || queue.state === 'staticLimitOnly'
    || queue.state === 'blocked';
}

function actionableGapQueueRepoMapSuppressedMessage(queue: RiprActionableGapQueueStatus): string {
  switch (queue.state) {
    case 'missing':
      return 'ripr actionable gap queue is not generated for this workspace, so there is no repo gap map to copy.';
    case 'malformed':
      return 'ripr actionable gap queue is malformed; repo gap map is suppressed.';
    case 'unsupportedSchema':
      return 'ripr actionable gap queue schema is unsupported; repo gap map is suppressed.';
    case 'wrongRoot':
      return 'ripr actionable gap queue belongs to another workspace; repo gap map is suppressed.';
    case 'unsafePath':
      return 'ripr actionable gap queue contains an unsafe path; repo gap map is suppressed.';
    case 'unsafeCommand':
      return 'ripr actionable gap queue contains an unsafe command; repo gap map is suppressed.';
    case 'noWorkspace':
      return 'Open a workspace before copying a ripr repo gap map.';
    case 'stale':
      return 'ripr actionable gap queue is stale; refresh saved-workspace evidence before copying a repo gap map.';
    case 'topActionableGap':
    case 'noAction':
    case 'reportOnly':
    case 'staticLimitOnly':
    case 'blocked':
      return 'ripr repo gap map is unavailable for the current queue state.';
  }
}

function repoGapMap(
  queue: RiprActionableGapQueueStatus,
  receipt: RiprReceiptArtifactStatus,
  firstPr: RiprFirstPrPacketStatus
): string {
  const lines = [
    'RIPR repo gap map',
    '',
    'Scope',
    `Artifact: ${queue.relativePath}`,
    `Queue state: ${queue.state}`,
    `Actionable gaps: ${queue.actionableGaps ?? 0}`,
    `Report-only gaps: ${queue.reportOnlyGaps ?? 0}`,
    `Static-limit-only gaps: ${queue.staticLimitOnlyGaps ?? 0}`,
    '',
    'Top queue item'
  ];
  if (queue.state === 'topActionableGap') {
    pushOptionalLine(lines, 'Canonical gap id', queue.canonicalGapId);
    pushOptionalLine(lines, 'Changed behavior', queue.changedBehavior);
    pushOptionalLine(lines, 'Why this matters', queue.why);
    pushOptionalLine(lines, 'Repair kind', queue.topRepair);
    pushOptionalLine(lines, 'Missing discriminator', queue.missingDiscriminator);
    pushOptionalLine(lines, 'Focused proof intent', queue.focusedProofIntent);
    pushOptionalLine(lines, 'Language', queue.languageStatus ? `${queue.language ?? 'unknown'} (${queue.languageStatus})` : queue.language);
    pushOptionalLine(lines, 'Related test', queue.relatedTest);
    pushOptionalLine(lines, 'Verify', queue.verifyCommand);
    pushOptionalLine(lines, 'Receipt command or path', queue.receiptCommandOrPath);
  } else {
    lines.push(`No top repair packet is available in state ${queue.state}.`);
    if (queue.detail) {
      lines.push(`Detail: ${queue.detail}`);
    }
  }
  lines.push('');
  lines.push('Receipt state');
  lines.push(`State: ${receipt.state}`);
  pushOptionalLine(lines, 'Movement', receipt.movement);
  pushOptionalLine(lines, 'Receipt seam', receipt.seamId);
  lines.push('');
  lines.push('First PR packet state');
  lines.push(`State: ${firstPr.state}`);
  pushOptionalLine(lines, 'First PR gap id', firstPr.canonicalGapId ?? firstPr.gapId);
  lines.push('');
  lines.push('Safe next commands');
  lines.push('- Refresh saved-workspace evidence before acting on stale queue state.');
  lines.push('- Regenerate actionable-gaps artifacts with the tooling that produced them.');
  lines.push('- Use ripr: Copy Current Repair Packet only for a validated top actionable gap.');
  lines.push('');
  lines.push('Non-claims');
  lines.push('- This map is read-only orientation.');
  lines.push('- It is not a gate decision, merge approval, runtime proof, mutation proof, coverage claim, or policy eligibility claim.');
  return lines.join('\n');
}

function pushOptionalLine(lines: string[], label: string, value: string | undefined): void {
  if (value) {
    lines.push(`${label}: ${value}`);
  }
}
export function uriFromTarget(target: RiprContextTarget | undefined): vscode.Uri | undefined {
  if (!target?.uri) {
    return undefined;
  }
  try {
    return vscode.Uri.parse(target.uri);
  } catch {
    return undefined;
  }
}

export function lineFromTarget(target: RiprContextTarget | undefined): number | undefined {
  if (typeof target?.line !== 'number' || !Number.isFinite(target.line) || target.line < 1) {
    return undefined;
  }
  return Math.floor(target.line);
}
interface AgentLoopCommandContract {
  targetArtifact?: string;
  // The exact command the server renders from this payload (`loop_commands.rs`
  // with `shell_arg` quoting), without the redirect; `undefined` when the
  // payload cannot have produced one. Equality leaves no room for an extra
  // token such as `$(cmd)` (#4225).
  expectedBody?: (target: RiprAgentLoopCommandTarget, commandRoot: string) => string | undefined;
  // Labels the payload does not pin down are checked by `<rootedPrefix> --root
  // <root>` and substrings, where `<root>` is `.` or a selected workspace
  // spelling (#4001).
  rootedPrefix?: string;
  includes?: string[];
  // The command ends in `> <targetArtifact>`. Since #3938 the server anchors
  // that redirect at the resolved `--root`, so the tail is checked by
  // the exact selected-root tail for fixed LSP actions.
  redirectsToTargetArtifact?: boolean;
}

const AGENT_LOOP_COMMAND_CONTRACTS: Record<string, AgentLoopCommandContract> = {
  // The server offers the repair start only for a seam `agent repair` would
  // accept (the fail-closed repair-packet flip, RIPR-SPEC-0087 §8, plus a
  // test-surface target; #3906); the attempt is retained under
  // target/ripr/repair-attempts.
  agent_repair: {
    targetArtifact: 'target/ripr/repair-attempts',
    expectedBody: withSeamId((seamId, commandRoot) => `ripr agent repair --root ${commandRoot} --seam-id ${seamId} --phase before`)
  },
  agent_packet: {
    targetArtifact: 'target/ripr/agent/agent-packet.json',
    expectedBody: withSeamId((seamId, commandRoot) => `ripr agent packet --root ${commandRoot} --seam-id ${seamId} --json`),
    redirectsToTargetArtifact: true
  },
  agent_brief: {
    targetArtifact: 'target/ripr/agent/agent-brief.json',
    expectedBody: withSeamId((seamId, commandRoot) => `ripr agent brief --root ${commandRoot} --seam-id ${seamId} --json`),
    redirectsToTargetArtifact: true
  },
  after_snapshot: {
    targetArtifact: 'target/ripr/pilot/after.repo-exposure.json',
    expectedBody: afterSnapshotBody,
    redirectsToTargetArtifact: true
  },
  agent_verify: {
    targetArtifact: 'target/ripr/agent/agent-verify.json',
    expectedBody: (_target, commandRoot) => `ripr agent verify --root ${commandRoot} --before target/ripr/pilot/repo-exposure.json --after target/ripr/pilot/after.repo-exposure.json --json`,
    redirectsToTargetArtifact: true
  },
  agent_receipt: {
    targetArtifact: 'target/ripr/agent/agent-receipt.json',
    expectedBody: withSeamId((seamId, commandRoot) =>
      `ripr agent receipt --root ${commandRoot} --verify-json target/ripr/agent/agent-verify.json --seam-id ${seamId} --json --out target/ripr/agent/agent-receipt.json`
    )
  },
  gap_verify: {
    rootedPrefix: 'ripr agent verify',
    includes: ['--json']
  },
  gap_receipt: {
    rootedPrefix: 'ripr agent receipt',
    includes: ['--json']
  }
};

function withSeamId(
  render: (seamId: string, commandRoot: string) => string
): (target: RiprAgentLoopCommandTarget, commandRoot: string) => string | undefined {
  return (target, commandRoot) =>
    boundedPayloadString(target.seam_id) ? render(serverShellArg(target.seam_id), commandRoot) : undefined;
}

// `check_repo_exposure_command_with_base`: `--base` appears exactly when the
// payload names one.
function afterSnapshotBody(target: RiprAgentLoopCommandTarget, commandRoot: string): string | undefined {
  const mode = target.mode;
  if (typeof mode !== 'string' || !['instant', 'draft', 'fast', 'deep', 'ready'].includes(mode)) {
    return undefined;
  }
  let base = '';
  if (target.base !== undefined && target.base !== null) {
    if (!boundedPayloadString(target.base)) {
      return undefined;
    }
    base = ` --base ${serverShellArg(target.base)}`;
  }
  return `ripr check --root ${commandRoot}${base} --mode ${mode} --format repo-exposure-json`;
}

export function validatedAgentLoopCommand(
  target: RiprAgentLoopCommandTarget | undefined,
  redirectRoots: readonly string[]
): string | undefined {
  if (!target) {
    return undefined;
  }
  const label = typeof target?.label === 'string' ? target.label : '';
  const contract = AGENT_LOOP_COMMAND_CONTRACTS[label];
  if (!contract) {
    return undefined;
  }
  const command = typeof target?.command === 'string' ? target.command.trim() : '';
  if (!command || target.root !== '.') {
    return undefined;
  }
  if (
    contract.targetArtifact !== undefined &&
    (typeof target.target_artifact !== 'string' ||
      target.target_artifact !== contract.targetArtifact)
  ) {
    return undefined;
  }
  if (contract.expectedBody) {
    // LSP copy actions bind the selected absolute root in the command, even
    // though the payload keeps its portable root role (`"."`). Compare the
    // entire server-rendered body and, when present, its redirect against the
    // SAME selected spelling. An absolute command with a relative or other-root
    // redirect is never equivalent (#4396).
    for (const { displayRoot, commandRoot } of selectedCommandRoots(redirectRoots)) {
      let body = command;
      if (contract.redirectsToTargetArtifact && contract.targetArtifact !== undefined) {
        const expectedTail = serverShellArg(
          `${displayRoot.replace(/\/$/, '')}/${contract.targetArtifact}`
        );
        if (!command.endsWith(` > ${expectedTail}`)) {
          continue;
        }
        body = command.slice(0, -` > ${expectedTail}`.length);
      }
      if (
        body === contract.expectedBody(target, commandRoot) &&
        !hasUnsafeShellMetacharacter(body.replace(`--root ${commandRoot}`, '--root .'), isWindowsHost())
      ) {
        return command;
      }
    }
    return undefined;
  }
  // Gap commands retain their separate, legacy prefix contract.
  let body = command;
  if (contract.redirectsToTargetArtifact && contract.targetArtifact !== undefined) {
    const redirectAt = command.lastIndexOf(' > ');
    if (
      redirectAt < 0 ||
      !redirectTargetMatches(
        command.slice(redirectAt + ' > '.length),
        contract.targetArtifact,
        redirectRoots,
        isWindowsHost()
      )
    ) {
      return undefined;
    }
    body = command.slice(0, redirectAt);
  }
  // Since #4001 the server binds the gap route's `--root .` to the selected
  // workspace; the legacy portable form is still accepted. The root token is
  // replaced by `.` before the metacharacter check, as for exact bodies, so a
  // workspace path's quoted `&` or `;` is not read as an operator.
  const rootArgs = ['.', ...selectedCommandRoots(redirectRoots).map(({ commandRoot }) => commandRoot)];
  const rootArg = rootArgs.find((candidate) => {
    const prefix = `${contract.rootedPrefix ?? ''} --root ${candidate}`;
    return body === prefix || body.startsWith(`${prefix} `);
  });
  if (rootArg === undefined) {
    return undefined;
  }
  const portableBody = body.replace(`--root ${rootArg}`, '--root .');
  if (hasUnsafeShellMetacharacter(portableBody, isWindowsHost())) {
    return undefined;
  }
  // One redirect only: a `>` here would truncate some other file.
  if (
    portableBody.includes('>') ||
    !(contract.includes ?? []).every((expected) => portableBody.includes(expected))
  ) {
    return undefined;
  }
  return command;
}

/**
 * The selected workspace spellings a bound command may name, each with the
 * `shell_arg` rendering the server writes after `--root`. Apostrophe and
 * backslash quoting is not portable across supported shells, so roots that
 * would need it, or that carry characters able to end or alter a quoted span,
 * are skipped.
 */
function selectedCommandRoots(
  redirectRoots: readonly string[]
): { displayRoot: string; commandRoot: string }[] {
  const roots: { displayRoot: string; commandRoot: string }[] = [];
  for (const root of redirectRoots) {
    if (!path.isAbsolute(root) || (process.platform !== 'win32' && root.includes('\\'))) {
      continue;
    }
    const displayRoot = path.normalize(root).replace(/\\/g, '/');
    if (/[\r\n\0`\\"'\u2018-\u201f]/.test(displayRoot)) {
      continue;
    }
    roots.push({ displayRoot, commandRoot: serverShellArg(displayRoot) });
  }
  return roots;
}

/**
 * Selected workspace spellings accepted for a bound command and redirect.
 * The server may render the real path when the workspace folder is a symlink.
 */
async function workspaceRedirectRoots(workspaceRoot: string | undefined): Promise<string[]> {
  if (!workspaceRoot) {
    return [];
  }
  try {
    return [workspaceRoot, await fs.realpath(workspaceRoot)];
  } catch {
    return [workspaceRoot];
  }
}

function boundedPayloadString(value: unknown): value is string {
  return typeof value === 'string' && value.length > 0 && value.length <= 256;
}

function nearestGapDiagnostic(editor: vscode.TextEditor): vscode.Diagnostic | undefined {
  const position = editor.selection.active;
  return vscode.languages
    .getDiagnostics(editor.document.uri)
    .filter(isRiprGapDiagnostic)
    .map((diagnostic) => ({
      diagnostic,
      containsSelection: diagnostic.range.contains(position),
      lineDistance: Math.abs(diagnostic.range.start.line - position.line),
      characterDistance: Math.abs(diagnostic.range.start.character - position.character)
    }))
    .sort((left, right) =>
      Number(right.containsSelection) - Number(left.containsSelection) ||
      left.lineDistance - right.lineDistance ||
      left.characterDistance - right.characterDistance
    )[0]?.diagnostic;
}

function containingGapDiagnostic(editor: vscode.TextEditor): vscode.Diagnostic | undefined {
  const position = editor.selection.active;
  return vscode.languages
    .getDiagnostics(editor.document.uri)
    .find((diagnostic) => isRiprGapDiagnostic(diagnostic) && diagnostic.range.contains(position));
}

function isRiprGapDiagnostic(diagnostic: vscode.Diagnostic): boolean {
  return diagnostic.source === 'ripr' && diagnosticCodeText(diagnostic).startsWith('ripr-gap-');
}

function diagnosticCodeText(diagnostic: vscode.Diagnostic): string {
  const code = diagnostic.code;
  if (typeof code === 'string' || typeof code === 'number') {
    return String(code);
  }
  if (code && typeof code === 'object' && 'value' in code) {
    return String(code.value);
  }
  return '';
}

function startRepairActions(actions: Array<vscode.CodeAction | vscode.Command>): StartRepairAction[] {
  const candidates: StartRepairAction[] = [];
  for (const action of actions) {
    const command = commandForAction(action);
    if (!command) {
      continue;
    }
    const title = action.title || command.title;
    const priority = startRepairActionPriority(title, command);
    if (priority === undefined) {
      continue;
    }
    candidates.push({ title, command, priority });
  }
  return candidates.sort((left, right) =>
    left.priority - right.priority || left.title.localeCompare(right.title)
  );
}

function commandForAction(action: vscode.CodeAction | vscode.Command): vscode.Command | undefined {
  const codeActionCommand = (action as vscode.CodeAction).command;
  if (codeActionCommand && typeof codeActionCommand !== 'string') {
    return codeActionCommand;
  }
  const command = (action as vscode.Command).command;
  return typeof command === 'string' ? action as vscode.Command : undefined;
}

function startRepairActionPriority(title: string, command: vscode.Command): number | undefined {
  if (
    command.command === 'ripr.copyContext' &&
    firstArgumentLabelIs(command, 'first_repair_packet')
  ) {
    return 0;
  }
  if (
    command.command === 'ripr.copyContext' &&
    (title === 'Agent handoff: copy Python packet' || firstArgumentLabelIs(command, 'python_agent_packet'))
  ) {
    return 1;
  }
  if (
    command.command === 'ripr.copyContext' &&
    (title === 'Inspect gap: copy repair packet' || firstArgumentLabelIs(command, 'gap_repair_packet'))
  ) {
    return 2;
  }
  if (command.command === 'ripr.openRelatedTest') {
    return 3;
  }
  if (command.command === 'ripr.copyAgentVerifyCommand' && firstArgumentLabelIs(command, 'gap_verify')) {
    return 4;
  }
  if (command.command === 'ripr.copyAgentReceiptCommand' && firstArgumentLabelIs(command, 'gap_receipt')) {
    return 5;
  }
  if (
    command.command === 'ripr.copyContext' &&
    (title === 'Inspect gap: copy static-limit note' || firstArgumentLabelIs(command, 'static_limit_note'))
  ) {
    return 6;
  }
  return undefined;
}

function firstArgumentLabelIs(command: vscode.Command, expected: string): boolean {
  const first = command.arguments?.[0];
  return Boolean(
    first &&
    typeof first === 'object' &&
    'label' in first &&
    (first as { label?: unknown }).label === expected
  );
}

async function pickStartRepairAction(actions: StartRepairAction[]): Promise<StartRepairAction | undefined> {
  const items = actions.map((action) => ({
    label: action.title,
    description: startRepairActionDescription(action.command),
    action
  }));
  const selected = await vscode.window.showQuickPick(items, {
    placeHolder: 'Start current ripr repair',
    matchOnDescription: true
  });
  return selected?.action;
}

function startRepairActionDescription(command: vscode.Command): string | undefined {
  switch (command.command) {
    case 'ripr.copyContext':
      if (firstArgumentLabelIs(command, 'first_repair_packet')) {
        return 'Copy the bounded packet';
      }
      if (firstArgumentLabelIs(command, 'python_agent_packet')) {
        return 'Copy the bounded Python agent packet';
      }
      return 'Copy the gap context';
    case 'ripr.openRelatedTest':
      return 'Open the likely repair target';
    case 'ripr.copyAgentVerifyCommand':
      return 'Copy the verify command';
    case 'ripr.copyAgentReceiptCommand':
      return 'Copy the receipt command';
    default:
      return undefined;
  }
}

export {
  actionableGapQueueSuppressedMessage,
  commandForAction,
  containingGapDiagnostic,
  firstArgumentLabelIs,
  nearestGapDiagnostic,
  workspaceRedirectRoots
};

export class AgentHandoffController {
  constructor(private readonly host: AgentHandoffHost) {}

  async copyContext(target?: RiprContextTarget): Promise<void> {
    const targetUri = uriFromTarget(target);
    const rootBlocker = this.host.copyActionRootBlocker(targetUri);
    if (rootBlocker) {
      this.host.runtime.showInformationMessage(rootBlocker);
      return;
    }
    const generation = this.host.getStartGeneration();

    const directPacketLabel = target?.label === 'first_repair_packet'
      ? 'first repair packet'
      : target?.label === 'gap_repair_packet'
        ? 'gap repair packet'
        : target?.label === 'repair_card'
          ? 'repair card'
          : undefined;
    if (directPacketLabel && target && typeof target.packet === 'string') {
      const packet = target.packet.trim();
      if (!packet) {
        this.host.runtime.showInformationMessage(`No ripr ${directPacketLabel} is available for this diagnostic.`);
        return;
      }
      try {
        await this.host.runtime.writeClipboard(packet);
        this.host.runtime.showInformationMessage(`Copied ripr ${directPacketLabel} to clipboard.`);
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        this.host.output.appendLine(`ripr copy ${directPacketLabel} failed: ${message}`);
        this.host.warnWithOutput(`ripr could not copy the ${directPacketLabel}.`);
      }
      return;
    }

    if (target?.label === 'static_limit_note' && typeof target.note === 'string') {
      const note = target.note.trim();
      if (!note) {
        this.host.runtime.showInformationMessage('No ripr static-limit note is available for this diagnostic.');
        return;
      }
      try {
        await this.host.runtime.writeClipboard(note);
        this.host.runtime.showInformationMessage('Copied ripr static-limit note to clipboard.');
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        this.host.output.appendLine(`ripr copy static-limit note failed: ${message}`);
        this.host.warnWithOutput('ripr could not copy the static-limit note.');
      }
      return;
    }

    const editor = vscode.window.activeTextEditor;
    const documentUri = targetUri ?? editor?.document.uri;
    if (!documentUri) {
      this.host.runtime.showInformationMessage('Open a Rust file before copying ripr context.');
      return;
    }

    const client = this.host.getClient();
    if (client && (target?.finding_id || target?.seam_id || target?.gap_id)) {
      try {
        const collectContextTarget: RiprContextTarget = {
          finding_id: target.finding_id,
          probe_id: target.probe_id,
          seam_id: target.seam_id,
          seam_kind: target.seam_kind,
          uri: target.uri,
          line: target.line,
        };
        if (target.evidence_identity !== undefined) {
          collectContextTarget.evidence_identity = target.evidence_identity;
        }
        if (target.gap_id) {
          collectContextTarget.gap_id = target.gap_id;
          collectContextTarget.canonical_gap_id = target.canonical_gap_id;
          collectContextTarget.gap_kind = target.gap_kind;
          collectContextTarget.gap_ledger = target.gap_ledger;
        }
        const packet = await client.sendRequest('workspace/executeCommand', {
          command: 'ripr.collectContext',
          arguments: [collectContextTarget],
        });
        if (!this.host.actionRootStillCurrent(generation, documentUri, (target) => this.host.copyActionRootBlocker(target))) {
          return;
        }
        if (packet && typeof packet === 'object') {
          await this.host.runtime.writeClipboard(JSON.stringify(packet, null, 2));
          this.host.runtime.showInformationMessage('Copied ripr context to clipboard.');
          return;
        }
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        this.host.output.appendLine(`ripr collectContext via LSP failed: ${message}`);
      }
    }

    if (!this.host.actionRootStillCurrent(generation, documentUri, (target) => this.host.copyActionRootBlocker(target))) {
      return;
    }
    const workspaceFolder = this.host.workspaceFolderForUri(documentUri);
    if (!workspaceFolder) {
      this.host.runtime.showInformationMessage('ripr context requires a workspace folder.');
      return;
    }

    const config = this.host.runtime.getConfig();
    const server = this.host.getServer() ?? await this.host.resolveServerForCommand(config);
    if (!server || !this.host.actionRootStillCurrent(generation, documentUri, (target) => this.host.copyActionRootBlocker(target))) {
      return;
    }
    const relativePath = path.relative(workspaceFolder.uri.fsPath, documentUri.fsPath);
    const activeLine = editor ? editor.selection.active.line + 1 : undefined;
    const line = lineFromTarget(target) ?? activeLine ?? 1;
    const selector = `${relativePath}:${line}`;
    // An empty base lets `ripr context` resolve the repository's default
    // branch the same way the language server does.
    const baseRef = config.baseRef.trim();
    const args = [
      'context',
      '--root',
      workspaceFolder.uri.fsPath,
      ...(baseRef ? ['--base', baseRef] : []),
      '--at',
      selector,
      '--json'
    ];

    try {
      const context = await this.host.runtime.runRipr(server.command, args, workspaceFolder.uri.fsPath);
      if (!this.host.actionRootStillCurrent(generation, documentUri, (target) => this.host.copyActionRootBlocker(target))) {
        return;
      }
      await this.host.runtime.writeClipboard(context.trim());
      this.host.runtime.showInformationMessage('Copied ripr context to clipboard.');
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      this.host.output.appendLine(`ripr context failed: ${message}`);
      this.host.warnWithOutput(`ripr context failed for ${selector}.`);
    }
  }

  async startCurrentRepair(): Promise<void> {
    const editor = vscode.window.activeTextEditor;
    if (!editor || !this.host.isRiprFileDocument(editor.document)) {
      this.host.runtime.showInformationMessage('Open a Rust, TypeScript/JavaScript, or Python file before starting a ripr repair.');
      return;
    }
    const rootBlocker = this.host.activeDocumentRootBlocker(editor.document);
    if (rootBlocker) {
      this.host.runtime.showInformationMessage(rootBlocker);
      return;
    }
    const setupBlocker = this.host.setupRepairBlocker();
    if (setupBlocker) {
      this.host.runtime.showInformationMessage(setupBlocker);
      return;
    }
    const diagnostic = nearestGapDiagnostic(editor);
    if (!diagnostic) {
      this.host.runtime.showInformationMessage('No current ripr repair gap is available near the active selection.');
      return;
    }

    try {
      await vscode.commands.executeCommand('editor.action.showHover');
    } catch {
      // Hover is an ergonomic hint only; code actions remain the source of truth.
    }

    let actions: Array<vscode.CodeAction | vscode.Command> | undefined;
    try {
      actions = await vscode.commands.executeCommand<Array<vscode.CodeAction | vscode.Command>>(
        'vscode.executeCodeActionProvider',
        editor.document.uri,
        diagnostic.range
      );
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      this.host.output.appendLine(`ripr start current repair failed to collect code actions: ${message}`);
      this.host.warnWithOutput('ripr could not collect current repair actions.');
      return;
    }

    const candidates = startRepairActions(actions ?? []);
    if (candidates.length === 0) {
      this.host.runtime.showInformationMessage('No bounded ripr repair action is available for the current gap. Refresh saved-workspace analysis if this looks stale.');
      return;
    }

    const selected = candidates.length === 1
      ? candidates[0]
      : await pickStartRepairAction(candidates);
    if (!selected) {
      return;
    }
    await vscode.commands.executeCommand(
      selected.command.command,
      ...(selected.command.arguments ?? [])
    );
  }

  async copySuggestedAssertion(target?: RiprSuggestedAssertionTarget): Promise<void> {
    if (!target) {
      this.host.runtime.showInformationMessage(DIAGNOSTIC_SCOPED_COMMAND_GUIDANCE);
      return;
    }
    const assertion = typeof target?.assertion === 'string' ? target.assertion.trim() : '';
    if (!assertion) {
      this.host.runtime.showInformationMessage('No ripr suggested assertion is available for this diagnostic.');
      return;
    }
    try {
      await this.host.runtime.writeClipboard(assertion);
      this.host.runtime.showInformationMessage('Copied ripr suggested assertion to clipboard.');
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      this.host.output.appendLine(`ripr copy suggested assertion failed: ${message}`);
      this.host.warnWithOutput('ripr could not copy the suggested assertion.');
    }
  }

  async copyTargetedTestBrief(target?: RiprTargetedTestBriefTarget): Promise<void> {
    if (!target) {
      this.host.runtime.showInformationMessage(DIAGNOSTIC_SCOPED_COMMAND_GUIDANCE);
      return;
    }
    const brief = typeof target?.brief === 'string' ? target.brief.trim() : '';
    if (!brief) {
      this.host.runtime.showInformationMessage('No ripr targeted test brief is available for this diagnostic.');
      return;
    }
    try {
      await this.host.runtime.writeClipboard(brief);
      this.host.runtime.showInformationMessage('Copied ripr targeted test brief to clipboard.');
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      this.host.output.appendLine(`ripr copy targeted test brief failed: ${message}`);
      this.host.warnWithOutput('ripr could not copy the targeted test brief.');
    }
  }

  async copyAgentLoopCommand(target?: RiprAgentLoopCommandTarget): Promise<void> {
    // Invoked from the palette without a diagnostic target: say what to do
    // instead of appearing to no-op (#2083). This must precede the root
    // blocker check, which would otherwise swallow the guidance when no
    // editor is active. Platform-neutral: Quick Fix is Ctrl+. on
    // Windows/Linux and Cmd+. on macOS.
    if (!target) {
      this.host.runtime.showInformationMessage(
        'ripr agent loop commands run from a ripr diagnostic: open a file with ripr diagnostics, use Quick Fix (Ctrl+. or Cmd+.) on one, and pick the agent command from there.'
      );
      return;
    }

    const rootBlocker = this.host.copyActionRootBlocker(uriFromTarget(target));
    if (rootBlocker) {
      this.host.runtime.showInformationMessage(rootBlocker);
      return;
    }

    const generation = this.host.getStartGeneration();
    const command = validatedAgentLoopCommand(target, await workspaceRedirectRoots(this.host.getWorkspaceRoot()));
    if (!this.host.actionRootStillCurrent(generation, uriFromTarget(target), (checkTarget) => this.host.copyActionRootBlocker(checkTarget))) {
      return;
    }
    if (!command) {
      this.host.runtime.showInformationMessage('No ripr agent loop command is available for this diagnostic.');
      return;
    }
    try {
      await this.host.runtime.writeClipboard(command);
      this.host.runtime.showInformationMessage('Copied ripr agent loop command to clipboard.');
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      this.host.output.appendLine(`ripr copy agent loop command failed: ${message}`);
      this.host.warnWithOutput('ripr could not copy the agent loop command.');
    }
  }

  async copyCurrentRepairPacket(): Promise<void> {
    if (!this.host.runtime.isWorkspaceTrusted()) {
      this.host.runtime.showInformationMessage('ripr workspace is not trusted; repair packets are unavailable.');
      return;
    }
    await this.host.refreshSetupStatusFiles();
    const queue = this.host.getSetupStatus().actionableQueue;
    if (this.host.getStatusKind() === 'stale' && actionableGapQueueCanBecomeStale(queue.state)) {
      this.host.runtime.showInformationMessage('ripr current repair packet requires current saved-workspace evidence; save or refresh first.');
      return;
    }
    if (!actionableGapQueueAllowsCurrentRepairPacket(queue)) {
      this.host.runtime.showInformationMessage(actionableGapQueueSuppressedMessage(queue));
      return;
    }
    try {
      await this.host.runtime.writeClipboard(currentRepairPacket(queue));
      this.host.runtime.showInformationMessage('ripr current repair packet copied.');
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      this.host.output.appendLine(`ripr copy current repair packet failed: ${message}`);
      this.host.warnWithOutput('ripr could not copy the current repair packet.');
    }
  }

  async copyRepoGapMap(): Promise<void> {
    await this.host.refreshSetupStatusFiles();
    const queue = this.host.getSetupStatus().actionableQueue;
    if (!actionableGapQueueAllowsRepoGapMap(queue)) {
      this.host.runtime.showInformationMessage(actionableGapQueueRepoMapSuppressedMessage(queue));
      return;
    }
    try {
      await this.host.runtime.writeClipboard(repoGapMap(queue, this.host.getSetupStatus().receipt, this.host.getSetupStatus().firstPr));
      this.host.runtime.showInformationMessage('ripr repo gap map copied.');
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      this.host.output.appendLine(`ripr copy repo gap map failed: ${message}`);
      this.host.warnWithOutput('ripr could not copy the repo gap map.');
    }
  }
  private async fetchWorkspaceCommand(
    command: string,
    failedLogLabel: string
  ): Promise<CockpitRequestResult<Record<string, unknown>>> {
    const client = this.host.getClient();
    if (!client) {
      return { kind: 'no_client' };
    }
    try {
      const response = await client.sendRequest('workspace/executeCommand', {
        command,
        arguments: []
      });
      return interpretCockpitObjectResponse(response);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      this.host.output.appendLine(`ripr ${failedLogLabel} failed: ${message}`);
      return { kind: 'unavailable' };
    }
  }

  private showCockpitUnavailable(
    result: CockpitRequestAbsence,
    unavailableMessage: string
  ): void {
    this.host.runtime.showInformationMessage(cockpitUnavailableMessage(result, unavailableMessage));
  }

  /** Shared helper: calls ripr.collectRepairPacket (top packet, no args) once. */
  private async fetchTopRepairPacket(): Promise<CockpitRequestResult<Record<string, unknown>>> {
    return this.fetchWorkspaceCommand('ripr.collectRepairPacket', 'collectRepairPacket');
  }

  async copyTopRepairPacket(): Promise<void> {
    const response = await this.fetchTopRepairPacket();
    if (response.kind !== 'ok') {
      this.showCockpitUnavailable(response, 'ripr server did not respond — no repair packet available.');
      return;
    }
    const packet = response.value;
    const status = typeof packet['status'] === 'string' ? packet['status'] : undefined;
    if (status === 'not_actionable_or_incomplete' || packet['kind'] !== 'repair_packet') {
      const reason = typeof packet['reason'] === 'string' ? packet['reason'] : undefined;
      // #4427: the server says whether a refresh is in flight (retry shortly)
      // or ended without a result (recover); surface its guidance.
      const recovery = typeof packet['recovery'] === 'string' ? packet['recovery'] : undefined;
      // Show only the hint when present; the raw reason token is not user text.
      this.host.runtime.showInformationMessage(
        recovery ?? reason ?? 'No complete repair packet available.',
      );
      return;
    }
    try {
      await this.host.runtime.writeClipboard(JSON.stringify(packet, null, 2));
      this.host.runtime.showInformationMessage('Copied top repair packet to clipboard.');
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      this.host.output.appendLine(`ripr copyTopRepairPacket clipboard write failed: ${message}`);
      this.host.warnWithOutput('ripr could not copy the repair packet.');
    }
  }

  async copyTopVerifyCommand(): Promise<void> {
    const response = await this.fetchTopRepairPacket();
    if (response.kind !== 'ok') {
      this.showCockpitUnavailable(response, 'No verify command available.');
      return;
    }
    const packet = response.value;
    const status = typeof packet['status'] === 'string' ? packet['status'] : undefined;
    if (status === 'not_actionable_or_incomplete' || packet['kind'] !== 'repair_packet') {
      this.host.runtime.showInformationMessage('No verify command available.');
      return;
    }
    const verifyCommand = typeof packet['verify_command'] === 'string'
      ? packet['verify_command'].trim()
      : '';
    if (!verifyCommand) {
      this.host.runtime.showInformationMessage('No verify command available.');
      return;
    }
    try {
      await this.host.runtime.writeClipboard(verifyCommand);
      this.host.runtime.showInformationMessage('Copied verify command to clipboard.');
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      this.host.output.appendLine(`ripr copyTopVerifyCommand clipboard write failed: ${message}`);
      this.host.warnWithOutput('ripr could not copy the verify command.');
    }
  }

  async copyTopReceiptCommand(): Promise<void> {
    const response = await this.fetchTopRepairPacket();
    if (response.kind !== 'ok') {
      this.showCockpitUnavailable(response, 'No receipt command available.');
      return;
    }
    const packet = response.value;
    const status = typeof packet['status'] === 'string' ? packet['status'] : undefined;
    if (status === 'not_actionable_or_incomplete' || packet['kind'] !== 'repair_packet') {
      this.host.runtime.showInformationMessage('No receipt command available.');
      return;
    }
    const receiptCommand = typeof packet['receipt_command'] === 'string'
      ? packet['receipt_command'].trim()
      : '';
    if (!receiptCommand) {
      this.host.runtime.showInformationMessage('No receipt command available.');
      return;
    }
    try {
      await this.host.runtime.writeClipboard(receiptCommand);
      this.host.runtime.showInformationMessage('Copied receipt command to clipboard.');
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      this.host.output.appendLine(`ripr copyTopReceiptCommand clipboard write failed: ${message}`);
      this.host.warnWithOutput('ripr could not copy the receipt command.');
    }
  }

  // ---------------------------------------------------------------------------
  // Receipt status / route-quality inspection commands (ripr.collectReceiptStatus)
  // ---------------------------------------------------------------------------

  /** Shared helper: calls ripr.collectReceiptStatus once and returns the response. */
  private async fetchReceiptStatus(): Promise<CockpitRequestResult<Record<string, unknown>>> {
    return this.fetchWorkspaceCommand('ripr.collectReceiptStatus', 'collectReceiptStatus');
  }

  async showReceiptStatus(): Promise<void> {
    const response = await this.fetchReceiptStatus();
    if (response.kind !== 'ok') {
      this.showCockpitUnavailable(response, 'No receipt status available — server not responding.');
      return;
    }
    const packet = response.value;
    const receiptStatus = typeof packet['receipt_status'] === 'string'
      ? packet['receipt_status']
      : 'not_available';
    const latestOutcome = typeof packet['latest_attempt_outcome'] === 'string'
      ? packet['latest_attempt_outcome']
      : 'not_available';
    const missingReason = typeof packet['missing_receipt_reason'] === 'string'
      ? packet['missing_receipt_reason']
      : 'not_available';

    this.host.output.appendLine('ripr receipt status:');
    this.host.output.appendLine(`  receipt_status: ${receiptStatus}`);
    this.host.output.appendLine(`  latest_attempt_outcome: ${latestOutcome}`);
    this.host.output.appendLine(`  missing_receipt_reason: ${missingReason}`);
    this.host.output.show();

    const summary = receiptStatus !== 'not_available'
      ? `ripr receipt status: ${receiptStatus}`
      : latestOutcome !== 'not_available'
        ? `ripr receipt status: not_available; latest outcome: ${latestOutcome}`
        : missingReason !== 'not_available'
          ? `ripr receipt status: not_available; reason: ${missingReason}`
          : 'ripr receipt status: not_available — no receipt artifact found.';
    this.host.runtime.showInformationMessage(summary);
  }

  async copyReceiptCommand(): Promise<void> {
    const response = await this.fetchReceiptStatus();
    if (response.kind !== 'ok') {
      this.showCockpitUnavailable(response, 'No receipt command is available — server not responding.');
      return;
    }
    const copyReceiptCommand = typeof response.value['copy_receipt_command'] === 'string'
      ? response.value['copy_receipt_command'].trim()
      : '';
    if (!copyReceiptCommand || copyReceiptCommand === 'not_available') {
      this.host.runtime.showInformationMessage('No receipt command is available for the current state.');
      return;
    }
    try {
      await this.host.runtime.writeClipboard(copyReceiptCommand);
      this.host.runtime.showInformationMessage('Copied receipt command to clipboard.');
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      this.host.output.appendLine(`ripr copyReceiptCommand clipboard write failed: ${message}`);
      this.host.warnWithOutput('ripr could not copy the receipt command.');
    }
  }

}

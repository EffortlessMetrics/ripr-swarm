import * as cp from 'child_process';

const DEFAULT_PROBE_TIMEOUT_MS = 5000;
const MAX_MESSAGE_BYTES = 1024 * 1024;

export type RequiredLspCapability =
  | 'textDocumentSync'
  | 'hover'
  | 'codeAction'
  | 'pullDiagnostics'
  | 'executeCommand'
  | 'workspaceFolders'
  | 'positionEncoding';
export type OptionalLspCapability = 'codeActionResolve' | 'workDoneProgress';

export const REQUIRED_SERVER_COMMANDS = [
  'ripr.refresh',
  'ripr.collectContext',
  'ripr.collectEvidenceContext',
  'ripr.collectWorkspaceStatus',
  'ripr.collectRepairPacket',
  'ripr.collectTopLimitation',
  'ripr.collectReceiptStatus'
] as const;

export interface LspCompatibilityEvidence {
  readonly status: 'compatible';
  readonly serverName?: string;
  readonly serverVersion?: string;
  readonly positionEncoding: 'utf-16';
  readonly required: Readonly<Record<RequiredLspCapability, true>>;
  readonly optional: Readonly<Record<OptionalLspCapability, boolean>>;
  readonly processResult: 'clean_exit';
}

export type LspProbeFailureKind =
  | 'spawn_failure'
  | 'framing_failure'
  | 'protocol_failure'
  | 'initialize_error'
  | 'initialize_timeout'
  | 'server_identity_mismatch'
  | 'missing_required_capability'
  | 'position_encoding_mismatch'
  | 'shutdown_failure'
  | 'process_failure';

export interface LspCompatibilityFailure {
  readonly status: 'incompatible';
  readonly kind: LspProbeFailureKind;
  readonly detail: string;
}

export type LspCompatibilityResult = LspCompatibilityEvidence | LspCompatibilityFailure;

interface JsonRpcMessage {
  readonly jsonrpc?: string;
  readonly id?: number | string | null;
  readonly method?: string;
  readonly result?: unknown;
  readonly error?: { readonly code?: number; readonly message?: string };
}

export async function probeStandardLspCompatibility(
  command: string,
  useShell = false,
  timeoutMs = DEFAULT_PROBE_TIMEOUT_MS
): Promise<LspCompatibilityResult> {
  return new Promise((resolve) => {
    let settled = false;
    let phase: 'initialize' | 'shutdown' | 'exit' = 'initialize';
    let evidence: Omit<LspCompatibilityEvidence, 'processResult'> | undefined;
    let stdout: Buffer<ArrayBufferLike> = Buffer.alloc(0);
    const child = cp.spawn(command, ['lsp', '--stdio'], {
      shell: useShell,
      detached: process.platform !== 'win32',
      stdio: ['pipe', 'pipe', 'pipe']
    });

    const finish = (result: LspCompatibilityResult, kill = true): void => {
      if (settled) {
        return;
      }
      settled = true;
      clearTimeout(timer);
      if (kill && child.exitCode === null && child.signalCode === null) {
        terminateProbeProcessTree(child);
      }
      resolve(result);
    };

    const fail = (kind: LspProbeFailureKind, detail: string): void => {
      finish({ status: 'incompatible', kind, detail });
    };

    const timer = setTimeout(() => {
      const kind = phase === 'initialize' ? 'initialize_timeout' : 'shutdown_failure';
      fail(kind, `Timed out after ${timeoutMs}ms during the LSP ${phase} phase.`);
    }, timeoutMs);

    child.once('error', (error) => fail('spawn_failure', error.message));
    child.stdin?.once('error', (error) => fail('process_failure', `LSP probe stdin failed: ${error.message}`));
    child.once('exit', (code, signal) => {
      if (settled) {
        return;
      }
      if (phase === 'exit' && code === 0 && evidence) {
        finish({ ...evidence, processResult: 'clean_exit' }, false);
        return;
      }
      fail(
        'process_failure',
        `LSP probe process exited during ${phase} with code ${String(code)} and signal ${String(signal)}.`
      );
    });

    child.stdout.on('data', (chunk: Buffer) => {
      if (settled) {
        return;
      }
      stdout = Buffer.concat([stdout, chunk]);
      try {
        stdout = consumeMessages(stdout, onMessage);
      } catch (error) {
        fail('framing_failure', error instanceof Error ? error.message : String(error));
      }
    });

    const onMessage = (message: JsonRpcMessage): void => {
      if (message.jsonrpc !== '2.0') {
        fail('protocol_failure', 'LSP response omitted the required JSON-RPC 2.0 envelope.');
        return;
      }
      if (message.method && message.id !== undefined) {
        writeMessage(child, { jsonrpc: '2.0', id: message.id, result: null });
        return;
      }
      if (phase === 'initialize' && message.id === 1) {
        if (!hasResponsePayload(message)) {
          fail('protocol_failure', 'Initialize response omitted both result and error.');
          return;
        }
        if (message.error) {
          fail('initialize_error', message.error.message ?? `Initialize failed with code ${String(message.error.code)}.`);
          return;
        }
        const checked = validateInitializeResult(message.result);
        if (checked.status === 'incompatible') {
          finish(checked);
          return;
        }
        evidence = checked;
        phase = 'shutdown';
        writeMessage(child, { jsonrpc: '2.0', method: 'initialized', params: {} });
        writeMessage(child, { jsonrpc: '2.0', id: 2, method: 'shutdown', params: null });
        return;
      }
      if (phase === 'shutdown' && message.id === 2) {
        if (!hasResponsePayload(message)) {
          fail('protocol_failure', 'Shutdown response omitted both result and error.');
          return;
        }
        if (message.error) {
          fail('shutdown_failure', message.error.message ?? 'The server rejected shutdown.');
          return;
        }
        phase = 'exit';
        writeMessage(child, { jsonrpc: '2.0', method: 'exit', params: null });
      }
    };

    writeMessage(child, {
      jsonrpc: '2.0',
      id: 1,
      method: 'initialize',
      params: {
        processId: process.pid,
        clientInfo: { name: 'ripr-vscode-compatibility-probe' },
        rootUri: null,
        capabilities: {
          general: { positionEncodings: ['utf-16'] },
          workspace: { configuration: true, workspaceFolders: true },
          textDocument: {
            hover: { contentFormat: ['markdown', 'plaintext'] },
            codeAction: { resolveSupport: { properties: ['edit', 'command'] } },
            diagnostic: {},
            publishDiagnostics: {}
          },
          window: { workDoneProgress: true }
        },
        workspaceFolders: null
      }
    });
  });
}

function consumeMessages(buffer: Buffer, consume: (message: JsonRpcMessage) => void): Buffer {
  let remaining = buffer;
  while (remaining.length > 0) {
    const headerEnd = remaining.indexOf('\r\n\r\n');
    if (headerEnd < 0) {
      if (remaining.length > 8192) {
        throw new Error('LSP response headers exceeded 8192 bytes or were not CRLF framed.');
      }
      return remaining;
    }
    const header = remaining.subarray(0, headerEnd).toString('ascii');
    const lengthMatch = /^content-length:\s*(\d+)\s*$/im.exec(header);
    if (!lengthMatch) {
      throw new Error('LSP response omitted a valid Content-Length header.');
    }
    const length = Number(lengthMatch[1]);
    if (!Number.isSafeInteger(length) || length > MAX_MESSAGE_BYTES) {
      throw new Error(`LSP response declared an invalid Content-Length of ${lengthMatch[1]}.`);
    }
    const bodyStart = headerEnd + 4;
    if (remaining.length < bodyStart + length) {
      return remaining;
    }
    const body = remaining.subarray(bodyStart, bodyStart + length).toString('utf8');
    let parsed: unknown;
    try {
      parsed = JSON.parse(body);
    } catch {
      throw new Error('LSP response body was not valid JSON.');
    }
    if (!isObject(parsed)) {
      throw new Error('LSP response body was not a JSON-RPC object.');
    }
    consume(parsed as JsonRpcMessage);
    remaining = remaining.subarray(bodyStart + length);
  }
  return remaining;
}

function validateInitializeResult(result: unknown): Omit<LspCompatibilityEvidence, 'processResult'> | LspCompatibilityFailure {
  if (!isObject(result) || !isObject(result.capabilities)) {
    return { status: 'incompatible', kind: 'initialize_error', detail: 'Initialize result omitted server capabilities.' };
  }
  const capabilities = result.capabilities;
  const missing: string[] = [];
  if (!supportedTextDocumentSync(capabilities.textDocumentSync)) missing.push('textDocumentSync');
  if (!providerEnabled(capabilities.hoverProvider)) missing.push('hoverProvider');
  if (!providerEnabled(capabilities.codeActionProvider)) missing.push('codeActionProvider');
  if (!providerEnabled(capabilities.diagnosticProvider)) missing.push('diagnosticProvider');
  const executeCommand = isObject(capabilities.executeCommandProvider)
    ? capabilities.executeCommandProvider
    : undefined;
  const commands = Array.isArray(executeCommand?.commands)
    ? executeCommand.commands.filter((command): command is string => typeof command === 'string')
    : [];
  const missingCommands = REQUIRED_SERVER_COMMANDS.filter((command) => !commands.includes(command));
  if (missingCommands.length > 0) missing.push(`executeCommandProvider.commands (${missingCommands.join(', ')})`);
  const workspace = isObject(capabilities.workspace) ? capabilities.workspace : undefined;
  const workspaceFolders = workspace && isObject(workspace.workspaceFolders) ? workspace.workspaceFolders : undefined;
  if (workspaceFolders?.supported !== true) missing.push('workspace.workspaceFolders.supported');
  if (missing.length > 0) {
    return {
      status: 'incompatible',
      kind: 'missing_required_capability',
      detail: `Initialize result omitted required capabilities: ${missing.join(', ')}.`
    };
  }
  const positionEncoding = capabilities.positionEncoding ?? 'utf-16';
  if (positionEncoding !== 'utf-16') {
    return {
      status: 'incompatible',
      kind: 'position_encoding_mismatch',
      detail: `Server selected ${String(positionEncoding)}; the extension requires utf-16.`
    };
  }
  const serverInfo = isObject(result.serverInfo) ? result.serverInfo : undefined;
  if (serverInfo?.name !== 'ripr' || typeof serverInfo.version !== 'string' || serverInfo.version.length === 0) {
    return {
      status: 'incompatible',
      kind: 'server_identity_mismatch',
      detail: 'Initialize result did not identify a versioned ripr server.'
    };
  }
  const codeAction = capabilities.codeActionProvider;
  return {
    status: 'compatible',
    serverName: serverInfo.name,
    serverVersion: serverInfo.version,
    positionEncoding: 'utf-16',
    required: {
      textDocumentSync: true,
      hover: true,
      codeAction: true,
      pullDiagnostics: true,
      executeCommand: true,
      workspaceFolders: true,
      positionEncoding: true
    },
    optional: {
      codeActionResolve: isObject(codeAction) && codeAction.resolveProvider === true,
      workDoneProgress:
        (isObject(capabilities.hoverProvider) && capabilities.hoverProvider.workDoneProgress === true) ||
        (isObject(codeAction) && codeAction.workDoneProgress === true)
    }
  };
}

function supportedTextDocumentSync(value: unknown): boolean {
  if (value === 1) {
    return true;
  }
  if (!isObject(value) || value.change !== 1) {
    return false;
  }
  return value.save === true || (isObject(value.save) && value.save.includeText !== false);
}

function hasResponsePayload(message: JsonRpcMessage): boolean {
  return Object.prototype.hasOwnProperty.call(message, 'result') || Object.prototype.hasOwnProperty.call(message, 'error');
}

function providerEnabled(value: unknown): boolean {
  return value === true || isObject(value);
}

function writeMessage(child: cp.ChildProcess, message: object): void {
  const body = Buffer.from(JSON.stringify(message), 'utf8');
  child.stdin?.write(`Content-Length: ${body.length}\r\n\r\n`);
  child.stdin?.write(body);
}

export function terminateProbeProcessTree(child: cp.ChildProcess): void {
  child.stdin?.destroy();
  if (child.pid && process.platform === 'win32') {
    cp.spawnSync('taskkill', ['/pid', String(child.pid), '/T', '/F'], {
      stdio: 'ignore',
      windowsHide: true
    });
    return;
  }
  if (child.pid) {
    try {
      process.kill(-child.pid, 'SIGKILL');
      return;
    } catch {
      // Fall through when the process exited between the currentness check and termination.
    }
  }
  child.kill('SIGKILL');
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

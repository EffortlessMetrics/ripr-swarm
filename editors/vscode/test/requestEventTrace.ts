import type * as vscode from 'vscode';
import type { CodeActionMiddleware } from 'vscode-languageclient/node';
import { boundedRequestEvidence, redactHostDiagnosticsText } from './hostDiagnostics';

export const MAX_REQUEST_TRACE_EVENTS = 64;
export const MAX_REQUEST_TRACE_BYTES = 16 * 1024;
type ProviderMiddleware = NonNullable<CodeActionMiddleware['provideCodeActions']>;
type DocumentIdentity = Pick<vscode.TextDocument, 'uri' | 'version'>;
type TraceKind = 'document.open' | 'document.change' | 'document.save' | 'document.close'
  | 'provider.begin' | 'provider.cancel' | 'provider.resolve' | 'provider.reject'
  | 'client.state' | 'process.spawn' | 'process.exit' | 'process.close'
  | 'command.begin' | 'command.resolve' | 'command.reject' | 'teardown.begin'
  | 'control.model-edit' | 'control.matching-provider-change' | 'control.neither';
interface Fields {
  document_id?: number; document_uri?: string; document_version?: number;
  invocation_id?: number; client_id?: number; client_pid?: number;
  content_changes?: number; trigger_kind?: number; cancelled?: boolean;
  old_state?: number; new_state?: number; exit_code?: number; signal?: string;
  range_start_line?: number; range_start_character?: number;
  range_end_line?: number; range_end_character?: number;
}

/** Local observations only: no wire IDs or inferred cancellation reason. */
export class RequestEventTrace {
  private readonly events: Record<string, unknown>[] = [];
  private readonly documents = new WeakMap<object, number>();
  private sequence = 0;
  private nextDocument = 0;
  private nextInvocation = 0;
  private dropped = 0;
  private readonly started: number;

  constructor(private readonly clock: () => number = () => performance.now()) {
    this.started = clock();
  }

  document(document: DocumentIdentity): Fields {
    if (!this.documents.has(document)) { this.documents.set(document, ++this.nextDocument); }
    return { document_id: this.documents.get(document), document_uri: document.uri.toString(),
      document_version: document.version };
  }

  record(kind: TraceKind, fields: Fields = {}, error?: unknown): void {
    // Observation failure must never replace the provider/test outcome.
    try {
      const safe = boundedRequestEvidence({ method: kind, document_uri: fields.document_uri,
        document_version: fields.document_version, client_id: fields.client_id,
        client_pid: fields.client_pid, range_start_line: fields.range_start_line,
        range_start_character: fields.range_start_character, range_end_line: fields.range_end_line,
        range_end_character: fields.range_end_character }, error);
      delete safe.method;
      for (const key of ['document_id', 'invocation_id', 'content_changes', 'trigger_kind',
        'old_state', 'new_state', 'exit_code'] as const) {
        const value = fields[key];
        if (typeof value === 'number' && Number.isFinite(value)) { safe[key] = value; }
      }
      if (typeof fields.cancelled === 'boolean') { safe.cancelled = fields.cancelled; }
      if (typeof fields.signal === 'string') { safe.signal = redactHostDiagnosticsText(fields.signal).slice(0, 256); }
      this.events.push({ sequence: ++this.sequence,
        elapsed_ms: Math.round((this.clock() - this.started) * 1000) / 1000, kind, ...safe });
      while (this.events.length > MAX_REQUEST_TRACE_EVENTS
        || Buffer.byteLength(JSON.stringify({ request_event_trace: this.snapshot() }, null, 2), 'utf8')
          > MAX_REQUEST_TRACE_BYTES) {
        this.events.shift(); this.dropped++;
      }
    } catch { this.dropped++; }
  }

  snapshot(): Record<string, unknown> {
    return { schema: 'ripr-request-event-trace-v1', dropped_events: this.dropped,
      events: this.events.map(event => ({ ...event })),
      claim_limit: 'Local observation IDs and delivery order; no LSP request IDs, global provider-set observation or cancellation-cause attribution.' };
  }

  middleware(previous?: ProviderMiddleware, client: Fields = {}): ProviderMiddleware {
    return (document, range, context, token, next) => {
      const fields = { ...this.document(document), client_id: client.client_id,
        client_pid: client.client_pid, invocation_id: ++this.nextInvocation,
        trigger_kind: context.triggerKind, range_start_line: range.start?.line,
        range_start_character: range.start?.character, range_end_line: range.end?.line,
        range_end_character: range.end?.character };
      this.record('provider.begin', { ...fields, cancelled: token.isCancellationRequested });
      const subscription = token.onCancellationRequested(() => {
        this.record('provider.cancel', { ...fields, cancelled: true });
      });
      let settled = false;
      const resolved = <T>(value: T): T => {
        if (!settled) {
          settled = true;
          subscription.dispose(); this.record('provider.resolve', fields);
        }
        return value;
      };
      const rejected = (error: unknown): never => {
        if (!settled) {
          settled = true;
          subscription.dispose(); this.record('provider.reject', fields, error);
        }
        throw error;
      };
      try {
        const result = previous ? previous(document, range, context, token, next)
          : next(document, range, context, token);
        if (result && typeof (result as PromiseLike<unknown>).then === 'function') {
          return (result as Thenable<(vscode.Command | vscode.CodeAction)[] | null | undefined>)
            .then(resolved, rejected);
        }
        return resolved(result);
      } catch (error) { return rejected(error); }
    };
  }
}

/** Subscribe without reading document contents; caller owns listener disposal. */
export function observeDocuments(
  workspace: Pick<typeof vscode.workspace, 'onDidOpenTextDocument' | 'onDidChangeTextDocument'
    | 'onDidSaveTextDocument' | 'onDidCloseTextDocument'>,
  trace: RequestEventTrace,
  matches: (document: DocumentIdentity) => boolean,
): vscode.Disposable[] {
  return [
    workspace.onDidOpenTextDocument(document => {
      if (matches(document)) { trace.record('document.open', trace.document(document)); }
    }),
    workspace.onDidChangeTextDocument(event => {
      if (matches(event.document)) { trace.record('document.change', {
        ...trace.document(event.document), content_changes: event.contentChanges.length }); }
    }),
    workspace.onDidSaveTextDocument(document => {
      if (matches(document)) { trace.record('document.save', trace.document(document)); }
    }),
    workspace.onDidCloseTextDocument(document => {
      if (matches(document)) { trace.record('document.close', trace.document(document)); }
    }),
  ];
}

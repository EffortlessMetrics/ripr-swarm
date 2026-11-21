import type * as vscode from 'vscode';
import type { CodeActionMiddleware } from 'vscode-languageclient/node';
import { boundedRequestEvidence, redactHostDiagnosticsText } from './hostDiagnostics';
import type { CodeActionEventSink } from './multiRootRequestDiagnostics';

export const MAX_REQUEST_TRACE_EVENTS = 64;
export const MAX_REQUEST_TRACE_BYTES = 16 * 1024;
type ProviderMiddleware = NonNullable<CodeActionMiddleware['provideCodeActions']>;
type DocumentIdentity = Pick<vscode.TextDocument, 'uri' | 'version'>;
type TraceKind = 'document.open' | 'document.change' | 'document.save' | 'document.close'
  | 'provider.begin' | 'provider.cancel' | 'provider.resolve' | 'provider.reject'
  | 'client.request.start' | 'client.request.cancel' | 'client.request.resolve' | 'client.request.reject'
  | 'client.observer.cleanup-failed'
  | 'client.state' | 'process.spawn' | 'process.exit' | 'process.close'
  | 'command.begin' | 'command.resolve' | 'command.reject' | 'teardown.begin'
  | 'control.model-edit' | 'control.matching-provider-change' | 'control.neither';
interface Fields {
  document_id?: number; document_uri?: string; document_version?: number;
  invocation_id?: number; client_id?: number; client_pid?: number;
  request_window_id?: number; client_request_id?: number; result_count?: number;
  content_changes?: number; trigger_kind?: number; cancelled?: boolean;
  old_state?: number; new_state?: number; exit_code?: number; signal?: string;
  range_start_line?: number; range_start_character?: number;
  range_end_line?: number; range_end_character?: number;
}

export interface ProviderObservation {
  status: 'observed' | 'missing' | 'ambiguous' | 'incomplete';
  matching_invocations?: number;
  invocation_id?: number;
  begin_sequence?: number;
  terminal_sequence?: number;
  terminal_kind?: 'provider.resolve' | 'provider.reject';
}

/** Local observations only: no wire IDs or inferred cancellation reason. */
export class RequestEventTrace {
  private readonly events: Record<string, unknown>[] = [];
  private readonly documents = new WeakMap<object, number>();
  private sequence = 0;
  private nextDocument = 0;
  private nextInvocation = 0;
  private nextRequestWindow = 0;
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
        'old_state', 'new_state', 'exit_code', 'request_window_id', 'client_request_id', 'result_count'] as const) {
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

  /** Client sendRequest ordinals are observations, not JSON-RPC wire IDs. */
  clientRequestEvents(client: { client_id: number }): CodeActionEventSink {
    const window = ++this.nextRequestWindow;
    return { push: event => {
      try {
        const kinds: Record<string, TraceKind> = {
          client_request_start: 'client.request.start', client_token_canceled: 'client.request.cancel',
          client_request_result: 'client.request.resolve', client_request_failed: 'client.request.reject',
          client_observer_cleanup_failed: 'client.observer.cleanup-failed',
        };
        const kind = typeof event.kind === 'string' && Object.hasOwn(kinds, event.kind)
          ? kinds[event.kind] : undefined;
        if (!kind) { return; } // Command/provider observations already have their own trace owner.
        const range = event.request_range as { start?: { line?: number; character?: number };
          end?: { line?: number; character?: number } } | undefined;
        const document = Array.isArray(event.document) && event.document.length === 1
          && event.request_uri === event.uri ? event.document[0] : undefined;
        let error: Error | undefined;
        if (kind === 'client.request.reject' || kind === 'client.observer.cleanup-failed') {
          error = new Error(typeof event.error_message === 'string' ? event.error_message
            : typeof event.error === 'string' ? event.error : '[failure description unavailable]');
          if (typeof event.error_name === 'string') { error.name = event.error_name; }
          error.stack = typeof event.error_stack === 'string' ? event.error_stack : undefined;
          Object.assign(error, { code: event.code });
        }
        this.record(kind, { client_id: client.client_id, request_window_id: window,
          client_request_id: event.request_id as number | undefined,
          client_pid: event.pid as number | undefined, document_uri: event.request_uri as string | undefined,
          document_version: document?.version, range_start_line: range?.start?.line,
          range_start_character: range?.start?.character, range_end_line: range?.end?.line,
          range_end_character: range?.end?.character, result_count: event.count as number | undefined,
          cancelled: kind === 'client.request.cancel' ? true : event.canceled as boolean | undefined }, error);
      } catch { this.dropped++; } // Even hostile metadata must not replace the original outcome.
    } };
  }

  /** Read immediately at command settlement, before later events can evict it.
   * This qualifies matching local observations, not command or wire identity.
   */
  observeProviderRequest(document: DocumentIdentity | undefined, range: vscode.Range,
    client: { client_id: number; client_pid?: number }): () => ProviderObservation {
    try {
      const after = this.sequence;
      const dropped = this.dropped;
      const retainedBefore = this.events.length;
      const target = document && { document_id: this.document(document).document_id,
        document_version: document.version, ...client,
        range_start_line: range.start.line, range_start_character: range.start.character,
        range_end_line: range.end.line, range_end_character: range.end.character,
        trigger_kind: 1 }; // CodeActionTriggerKind.Invoke; no runtime vscode import.
      return () => {
        try {
          // Evicting old entries does not lose this window. Any other drop
          // (including an observer failure before sequence allocation) does.
          const evictedBefore = retainedBefore
            - this.events.filter(event => Number(event.sequence) <= after).length;
          if (!target || this.dropped - dropped > evictedBefore) { return { status: 'incomplete' }; }
          const matches = (event: Record<string, unknown>) =>
            Object.entries(target).every(([key, value]) => event[key] === value);
          const begins = this.events.filter(event => event.kind === 'provider.begin'
            && Number(event.sequence) > after && matches(event));
          if (begins.length !== 1) {
            return { status: begins.length === 0 ? 'missing' : 'ambiguous',
              matching_invocations: begins.length };
          }
          const begin = begins[0];
          const terminals = this.events.filter(event =>
            (event.kind === 'provider.resolve' || event.kind === 'provider.reject')
            && event.invocation_id === begin.invocation_id
            && Number(event.sequence) > Number(begin.sequence) && matches(event));
          return { status: terminals.length === 1 ? 'observed' : 'incomplete',
            matching_invocations: 1, invocation_id: Number(begin.invocation_id),
            begin_sequence: Number(begin.sequence),
            ...(terminals.length === 1 ? {
              terminal_sequence: Number(terminals[0].sequence),
              terminal_kind: terminals[0].kind as 'provider.resolve' | 'provider.reject',
            } : {}) };
        } catch { return { status: 'incomplete' }; }
      };
    } catch { return () => ({ status: 'incomplete' }); }
  }

  middleware(previous?: ProviderMiddleware,
    client: Omit<Fields, 'client_pid'> & { client_pid?: number | (() => number | undefined) } = {}): ProviderMiddleware {
    return (document, range, context, token, next) => {
      const fields = { ...this.document(document), client_id: client.client_id,
        client_pid: typeof client.client_pid === 'function' ? client.client_pid() : client.client_pid,
        invocation_id: ++this.nextInvocation,
        trigger_kind: context.triggerKind, range_start_line: range.start?.line,
        range_start_character: range.start?.character, range_end_line: range.end?.line,
        range_end_character: range.end?.character };
      this.record('provider.begin', { ...fields, cancelled: token.isCancellationRequested });
      const subscription = token.onCancellationRequested(() => {
        this.record('provider.cancel', { ...fields, cancelled: true });
      });
      let settled = false;
      const settle = (kind: 'provider.resolve' | 'provider.reject', error?: unknown): void => {
        if (settled) { return; }
        settled = true;
        try { subscription.dispose(); } catch { /* disposal must not replace the outcome */ }
        this.record(kind, fields, error);
      };
      const resolved = <T>(value: T): T => { settle('provider.resolve'); return value; };
      const rejected = (error: unknown): never => { settle('provider.reject', error); throw error; };
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

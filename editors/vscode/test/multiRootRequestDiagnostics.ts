import { redactHostDiagnosticsText } from './hostDiagnostics';

export interface CodeActionEventSink {
  push(event: Record<string, unknown>): unknown;
}

/** Observe client invocation, not proof of transport transmission. sendRequest
 * can still await startup/document synchronization before writing a frame.
 * Actual params and an observer ordinal (not a JSON-RPC id) distinguish automatic actions
 * from the selected provider call without changing requests or cancellation.
 */
export function observeCodeActionRequests<T extends { sendRequest: (...args: never[]) => unknown }>(
  client: T,
  observe: () => Record<string, unknown>,
  events: CodeActionEventSink
): () => void {
  const original = client.sendRequest;
  let nextRequestId = 0;
  const subscriptions: Array<{ dispose(): void }> = [];
  client.sendRequest = (function (...args: unknown[]) {
    const type = args[0] as string | { method?: string };
    const method = typeof type === 'string' ? type : type?.method;
    if (method !== 'textDocument/codeAction') {
      return Reflect.apply(original, client, args);
    }
    const params = args[1] as { textDocument?: { uri?: string }; range?: unknown } | undefined;
    const request = { request_id: ++nextRequestId, method,
      request_uri: params?.textDocument?.uri, request_range: params?.range };
    const token = args.find(value => value !== null && typeof value === 'object'
      && 'isCancellationRequested' in value) as {
        isCancellationRequested: boolean;
        onCancellationRequested(listener: () => void): { dispose(): void };
      } | undefined;
    events.push({ ...safeObservation(observe), ...request, kind: 'client_request_start', canceled: token?.isCancellationRequested });
    if (token) {
      subscriptions.push(token.onCancellationRequested(() =>
        events.push({ ...safeObservation(observe), ...request, kind: 'client_token_canceled' })));
    }
    const rejected = (error: unknown): never => {
      events.push({ ...safeObservation(observe), ...request, kind: 'client_request_failed',
        ...describeCodeActionFailure(error) });
      throw error;
    };
    let result: unknown;
    try { result = Reflect.apply(original, client, args); }
    catch (error) { return rejected(error); }
    return Promise.resolve(result).then(value => {
      events.push({ ...safeObservation(observe), ...request, kind: 'client_request_result', count: Array.isArray(value) ? value.length : null });
      return value;
    }, rejected);
  }) as T['sendRequest'];
  return () => {
    client.sendRequest = original;
    for (const subscription of subscriptions.splice(0)) {
      try { subscription.dispose(); }
      catch (error) {
        events.push({ kind: 'client_observer_cleanup_failed', ...describeCodeActionFailure(error) });
      }
    }
  };
}

/** Each original failure field stays bounded before console/receipt retention. */
export const MAX_CODE_ACTION_FAILURE_FIELD_CHARS = 4096;

export function describeCodeActionFailure(error: unknown): Record<string, unknown> {
  const field = (key: string): unknown => {
    try {
      return error !== null && (typeof error === 'object' || typeof error === 'function')
        ? (error as Record<string, unknown>)[key] : undefined;
    } catch { return undefined; }
  };
  const text = (value: unknown): string | undefined => typeof value === 'string'
    ? redactHostDiagnosticsText(value).slice(0, MAX_CODE_ACTION_FAILURE_FIELD_CHARS) : undefined;
  let description: string;
  try { description = redactHostDiagnosticsText(String(error)).slice(0, MAX_CODE_ACTION_FAILURE_FIELD_CHARS); }
  catch { description = '[failure description unavailable]'; }
  const code = field('code');
  return {
    error: description, error_name: text(field('name')), error_message: text(field('message')),
    error_stack: text(field('stack')), code: typeof code === 'number' ? code : text(code)
  };
}

function safeObservation(observe: () => Record<string, unknown>): Record<string, unknown> {
  // Read getters inside the guard too: diagnostic metadata cannot replace the
  // result or original failure of the request/provider being observed.
  try { return { ...observe() }; }
  catch { return { observation_unavailable: true }; }
}

export async function observeCodeActionProvider<T>(
  run: () => PromiseLike<T> | T,
  observe: () => Record<string, unknown>,
  events: CodeActionEventSink
): Promise<T> {
  events.push({ ...safeObservation(observe), kind: 'provider_start' });
  try { return await run(); }
  catch (error) {
    events.push({ ...safeObservation(observe), kind: 'provider_failed', ...describeCodeActionFailure(error) });
    throw error;
  } finally { events.push({ ...safeObservation(observe), kind: 'provider_end' }); }
}

/** One original command and one bounded observation window; never a retry. */
export async function observeCodeActionRequestWindow<T>(
  client: { sendRequest: (...args: never[]) => unknown },
  run: () => PromiseLike<T> | T,
  observe: () => Record<string, unknown>,
  events: CodeActionEventSink
): Promise<T> {
  const restore = observeCodeActionRequests(client, observe, events);
  try { return await observeCodeActionProvider(run, observe, events); }
  finally { restore(); }
}

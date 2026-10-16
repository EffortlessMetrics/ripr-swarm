/** Observe client invocation, not proof of transport transmission. sendRequest
 * can still await startup/document synchronization before writing a frame.
 * Actual params and a per-invocation id distinguish concurrent automatic actions
 * from the selected provider call without changing requests or cancellation.
 */
export function observeCodeActionRequests<T extends { sendRequest: (...args: never[]) => unknown }>(
  client: T,
  observe: () => Record<string, unknown>,
  events: Record<string, unknown>[]
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
    events.push({ ...observe(), ...request, kind: 'client_request_start', canceled: token?.isCancellationRequested });
    if (token) {
      subscriptions.push(token.onCancellationRequested(() =>
        events.push({ ...observe(), ...request, kind: 'client_token_canceled' })));
    }
    return Promise.resolve(Reflect.apply(original, client, args)).then(value => {
      events.push({ ...observe(), ...request, kind: 'client_request_result', count: Array.isArray(value) ? value.length : null });
      return value;
    }, (error: unknown) => {
      const code = error !== null && typeof error === 'object' && 'code' in error ? error.code : undefined;
      events.push({ ...observe(), ...request, kind: 'client_request_failed', error: String(error), code });
      throw error;
    });
  }) as T['sendRequest'];
  return () => {
    client.sendRequest = original;
    subscriptions.splice(0).forEach(subscription => subscription.dispose());
  };
}

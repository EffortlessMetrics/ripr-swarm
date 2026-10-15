/** Transparent, bounded-scope observation of the real LanguageClient request. */
export function observeCodeActionRequests<T extends { sendRequest: (...args: never[]) => unknown }>(
  client: T,
  observe: () => Record<string, unknown>,
  events: Record<string, unknown>[]
): () => void {
  const original = client.sendRequest;
  const subscriptions: Array<{ dispose(): void }> = [];
  client.sendRequest = (function (...args: unknown[]) {
    const type = args[0] as string | { method?: string };
    const method = typeof type === 'string' ? type : type?.method;
    if (method !== 'textDocument/codeAction') {
      return Reflect.apply(original, client, args);
    }
    const token = args.find(value => value !== null && typeof value === 'object'
      && 'isCancellationRequested' in value) as {
        isCancellationRequested: boolean;
        onCancellationRequested(listener: () => void): { dispose(): void };
      } | undefined;
    events.push({ kind: 'wire_code_action', canceled: token?.isCancellationRequested, ...observe() });
    if (token) {
      subscriptions.push(token.onCancellationRequested(() =>
        events.push({ kind: 'wire_cancellation', ...observe() })));
    }
    return Promise.resolve(Reflect.apply(original, client, args)).then(value => {
      events.push({ kind: 'wire_result', count: Array.isArray(value) ? value.length : null, ...observe() });
      return value;
    }, (error: unknown) => {
      const code = error !== null && typeof error === 'object' && 'code' in error ? error.code : undefined;
      events.push({ kind: 'wire_failed', error: String(error), code, ...observe() });
      throw error;
    });
  }) as T['sendRequest'];
  return () => {
    client.sendRequest = original;
    subscriptions.splice(0).forEach(subscription => subscription.dispose());
  };
}

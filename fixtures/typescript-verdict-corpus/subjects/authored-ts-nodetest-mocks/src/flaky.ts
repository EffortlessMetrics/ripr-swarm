export function timeoutMs(attempt: number): number {
  return 100 * 2 ** attempt;
}

export function describeAttempt(attempt: number): string {
  return attempt === 0 ? "first" : `retry ${attempt}`;
}

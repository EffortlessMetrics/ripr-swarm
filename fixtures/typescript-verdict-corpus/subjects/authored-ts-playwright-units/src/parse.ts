export function parseCount(text: string): number {
  const n = Number.parseInt(text, 10);
  if (Number.isNaN(n)) {
    throw new Error(`not a count: ${text}`);
  }
  return n;
}

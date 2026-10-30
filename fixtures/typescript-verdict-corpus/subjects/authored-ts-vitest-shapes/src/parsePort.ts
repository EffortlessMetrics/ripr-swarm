export async function parsePort(text: string): Promise<number> {
  const n = Number(text);
  if (!Number.isInteger(n) || n < 1 || n > 65535) {
    throw new RangeError("bad port");
  }
  return n;
}

export function isRawNetworkError(e: { code?: string }): boolean {
  return e.code === "ECONNRESET";
}

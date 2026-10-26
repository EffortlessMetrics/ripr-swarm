export function isHopNetworkError(e: { code?: string }): boolean {
  return e.code === "ECONNRESET";
}

export function parseMemo(s: string): string {
  if (s === "") {
    throw new Error("memo required");
  }
  return s.trim();
}

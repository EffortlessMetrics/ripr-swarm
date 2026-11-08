const String = (x: unknown): string => "S" + x;

export function fmt(input: unknown): string {
  return typeof input !== "object" ? JSON.stringify(input) : String(input);
}

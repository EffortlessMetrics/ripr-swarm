export function withoutBase(input: string, base: string): string {
  if (!base || base === "/") {
    return input;
  }
  if (!input.startsWith(base)) {
    return input;
  }
  return input.slice(base.length);
}

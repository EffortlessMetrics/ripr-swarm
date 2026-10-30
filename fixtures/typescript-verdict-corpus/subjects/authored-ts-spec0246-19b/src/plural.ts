export function plural(n: number): string {
  return `${n} item${n === 1 ? "" : "s"}`;
}

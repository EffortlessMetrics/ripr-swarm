export function pick(
  name: string,
  opts: { level: string } = { level: 'AAA' },
): string {
  return name + opts.level;
}

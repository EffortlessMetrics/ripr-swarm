export function title(opts: { name?: string }): string {
  opts.name = 'Bob';
  return opts.name || 'untitled';
}

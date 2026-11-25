export function label(name: string | null, other = (name = 'Bob')): string {
  return name || 'anon';
}

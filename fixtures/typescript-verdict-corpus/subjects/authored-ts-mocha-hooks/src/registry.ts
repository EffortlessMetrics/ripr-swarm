const names: string[] = [];

export function register(name: string): number {
  names.push(name.trim());
  return names.length;
}

export function registered(): readonly string[] {
  return names;
}

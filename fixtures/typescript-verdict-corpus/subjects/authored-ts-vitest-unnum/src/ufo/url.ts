export function hasProtocol(input: string): boolean {
  return /^[a-z]+:\/\//.test(input);
}

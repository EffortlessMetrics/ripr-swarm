export function decodeThing(text: string): string {
  return decodeURIComponent(text.replace(/%252F/g, ""));
}

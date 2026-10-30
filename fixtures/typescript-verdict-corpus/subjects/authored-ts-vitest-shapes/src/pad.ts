export function pad(text: string, width = 8, fill = " "): string {
  return text.padStart(width, fill);
}

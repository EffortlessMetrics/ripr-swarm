export class ParseError extends Error {}

export function parseUserClass(input: string): string {
  if (input === "") {
    throw new ParseError("empty user");
  }
  return input;
}

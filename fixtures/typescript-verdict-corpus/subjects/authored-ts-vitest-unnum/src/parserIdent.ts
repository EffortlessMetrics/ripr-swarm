export function parseUserIdent(input: string): string {
  if (input === "") {
    throw new Error("empty user");
  }
  return input;
}

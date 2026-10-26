export function parseUserCode(input: string): string {
  if (input === "") {
    throw { code: "EMPTY_INPUT" };
  }
  return input;
}

export async function load(options?: { token?: string }): Promise<string> {
  if (!options?.token) {
    throw "TOKEN_REQUIRED";
  }
  return "ready";
}

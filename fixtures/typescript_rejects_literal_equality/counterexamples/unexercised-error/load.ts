export async function load(options?: { token?: string }): Promise<string> {
  if (!options?.token) {
    throw "TOKEN_REQUIRED";
  }
  if (options.token === "expired") {
    throw "TOKEN_EXPIRED";
  }
  return "ready";
}

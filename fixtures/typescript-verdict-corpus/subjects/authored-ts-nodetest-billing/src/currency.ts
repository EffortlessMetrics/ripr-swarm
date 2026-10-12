export function currencyCode(code: string): string {
  if (code !== "EUR" && code !== "USD") {
    throw new Error("unsupported currency");
  }
  return code;
}

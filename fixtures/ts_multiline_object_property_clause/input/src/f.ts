export function f(input: unknown, flag: boolean) {
  return {
    callable: flag && typeof input !== "function",
  };
}

export function f(input: unknown, flag: boolean): unknown {
  return typeof input !== "function" &&
    flag;
}

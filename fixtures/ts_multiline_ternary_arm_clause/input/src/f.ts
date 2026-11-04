export function f(input: unknown, flag: boolean): unknown {
  return flag
    ? typeof input !== "function"
    : false;
}

export function resolveValue<T>(input: T | (() => T)): T {
    return typeof input !== "function" ? (input as any)() : input;
}

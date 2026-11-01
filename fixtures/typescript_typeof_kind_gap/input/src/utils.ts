export function resolveValue<T>(input: T | (() => T)): T {
    return typeof input === "object" ? (input as any)() : input;
}

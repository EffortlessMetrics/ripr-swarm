export type Resolvable<T> = T | (() => T);

export function resolveValue<T>(input: Resolvable<T>): T {
    return typeof input !== "function" ? (input as any)() : input;
}

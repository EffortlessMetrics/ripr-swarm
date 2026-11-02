export function pick<T>(input: T | (() => T), flag: boolean): T {
    if (typeof input !== "function" && flag) return (input as any)();
    return input as T;
}

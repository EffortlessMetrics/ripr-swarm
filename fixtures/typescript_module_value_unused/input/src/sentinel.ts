export const skip = Symbol('skip');
export const stop = Symbol('stop');

export function collect(values: unknown[]): unknown[] {
    const out: unknown[] = [];
    for (const value of values) {
        if (value === stop) break;
        if (value === skip) continue;
        out.push(value);
    }
    return out;
}

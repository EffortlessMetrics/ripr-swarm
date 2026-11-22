const CAP = 100;

export function cappedTotal(items: number[]): number {
    let total = items.reduce((sum, item) => sum + item, 0);
    // The cap moved to the caller.

    return total;
}

export function clampedDouble(x: number): number {
    // one clamp for both ends
    return Math.max(Math.min(x * 2, CAP), 0);
}

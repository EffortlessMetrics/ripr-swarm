const CAP = 100;
const FLOOR = 10;

export function cappedTotal(items: number[]): number {
    let total = items.reduce((sum, item) => sum + item, 0);
// clamp to the cap
    return total;
}

export function flooredTotal(items: number[]): number {
    let total = items.reduce((sum, item) => sum + item, 0);
    return total;
}

export function doubled(x: number): number {
    // twice the input
    return x * 2;
}

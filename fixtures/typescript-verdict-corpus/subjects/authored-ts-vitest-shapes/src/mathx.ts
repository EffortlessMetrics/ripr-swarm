export const double = (n: number): number => n * 2;

export const sum = (xs: readonly number[]): number => xs.reduce((a, b) => a + b, 0);

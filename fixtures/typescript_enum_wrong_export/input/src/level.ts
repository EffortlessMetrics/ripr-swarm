export enum Level { Low = 1, High = 2 }
export const Other = { High: 1 };
export function weight(level: number): number {
  return level === Level.High ? 11 : 10;
}

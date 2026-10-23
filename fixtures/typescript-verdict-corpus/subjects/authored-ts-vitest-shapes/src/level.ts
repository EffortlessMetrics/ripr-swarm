export enum Level {
  Low = 1,
  High = 2,
}

export function weight(level: Level): number {
  return level === Level.High ? 10 : 1;
}

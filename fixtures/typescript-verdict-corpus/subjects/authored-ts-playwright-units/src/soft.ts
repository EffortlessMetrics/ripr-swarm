export function dims(w: number, h: number): { area: number; perimeter: number } {
  return { area: w * h, perimeter: 2 * (w + h) };
}

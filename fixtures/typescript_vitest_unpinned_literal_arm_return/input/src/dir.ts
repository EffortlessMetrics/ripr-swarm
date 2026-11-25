export function dir(n: number): string {
  switch (n) {
    case 1:
      return 'uno';
    case 2:
      return 'two';
    default:
      return 'other';
  }
}

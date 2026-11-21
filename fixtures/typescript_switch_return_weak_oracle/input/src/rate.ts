export function rate(kind: string): number {
  switch (kind) {
    case 'gold':
      return 10;
    case 'silver':
      return 7;
    default:
      return 0;
  }
}

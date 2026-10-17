export function statementTotals(days: number): { days: number; fee: number } {
  return { days, fee: days * 2 };
}

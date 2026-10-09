export interface Line {
  sku: string;
  unitCents: number;
  quantity: number;
}

export interface Notifier {
  send(owner: string, message: string): void;
}

export function lineTotal(line: Line): number {
  return line.unitCents * line.quantity;
}

export function bulkDiscount(quantity: number): number {
  if (quantity >= 10) {
    return 10;
  }
  return 0;
}

export function addLine(lines: Line[], line: Line): Line[] {
  if (line.quantity <= 0) {
    throw new RangeError("quantity must be positive");
  }
  return [...lines, line];
}

export function removeLine(lines: Line[], sku: string): Line[] {
  const kept = lines.filter((line) => line.sku !== sku);
  if (kept.length === lines.length) {
    throw new Error(`no line for ${sku}`);
  }
  return kept;
}

export function isEmpty(lines: Line[]): boolean {
  return lines.length === 0;
}

export function checkout(owner: string, lines: Line[], notifier: Notifier): number {
  const total = lines.reduce((sum, line) => sum + lineTotal(line), 0);
  notifier.send(owner, `charged ${total}`);
  return total;
}

export async function fetchUnitPrice(sku: string): Promise<number> {
  const table: Record<string, number> = { apple: 120, pear: 95 };
  return table[sku] ?? 0;
}

export function summary(lines: Line[]): string {
  return `${lines.length} items`;
}

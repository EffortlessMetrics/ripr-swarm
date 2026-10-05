export interface LineItem {
  sku: string;
  qty: number;
  total: number;
}

export function lineItem(sku: string, qty: number, unitCents: number): LineItem {
  return {
    sku,
    qty,
    total: qty * unitCents,
  };
}

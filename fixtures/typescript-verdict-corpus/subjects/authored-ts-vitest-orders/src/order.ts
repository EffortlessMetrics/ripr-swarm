export interface OrderLine {
  sku: string;
  qty: number;
  total: number;
}

export function buildOrderLine(sku: string, qty: number, unitPrice: number): OrderLine {
  return {
    sku,
    qty,
    total: qty * unitPrice,
  };
}

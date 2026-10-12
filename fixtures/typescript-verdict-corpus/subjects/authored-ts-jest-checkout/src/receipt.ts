export interface Receipt {
  id: number;
  total: number;
  currency: string;
}

export function buildReceipt(id: number, total: number): Receipt {
  return {
    id,
    total,
    currency: "USD",
  };
}

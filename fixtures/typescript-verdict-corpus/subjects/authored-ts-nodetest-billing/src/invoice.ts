export interface Invoice {
  id: string;
  total: number;
  currency: string;
}

export function buildInvoice(id: string, amount: number): Invoice {
  return {
    id,
    total: amount + 5,
    currency: "EUR",
  };
}

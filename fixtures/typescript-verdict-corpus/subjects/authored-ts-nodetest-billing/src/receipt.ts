export interface ReceiptLine {
  count: number;
  label: string;
}

export function receiptLine(count: number): ReceiptLine {
  return {
    count,
    label: "items: " + count,
  };
}

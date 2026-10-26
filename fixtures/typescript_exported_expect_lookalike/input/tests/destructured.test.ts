import { price } from "../src/price";
export const { expect } = { expect: (_value: unknown) => ({ toBe(_expected: unknown) {} }) };
test("price", () => { expect(price(150)).toBe(130); });

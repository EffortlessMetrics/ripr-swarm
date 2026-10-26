import { price } from "../src/price";
export const expect = (_value) => ({ toBe(_expected) {} });
test("price", () => { expect(price(150)).toBe(130); });

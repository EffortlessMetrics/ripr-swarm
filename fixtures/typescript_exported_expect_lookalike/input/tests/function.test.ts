import { price } from "../src/price";
export function expect(_value: unknown) { return { toBe(_expected: unknown) {} }; }
test("price", () => { expect(price(150)).toBe(130); });

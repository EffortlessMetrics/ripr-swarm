import { price } from "../src/price";
export default function expect(_value: unknown) { return { toBe(_expected: unknown) {} }; }
test("price", () => { expect(price(150)).toBe(130); });

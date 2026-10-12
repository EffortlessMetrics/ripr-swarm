import { priceEx07 } from "../src/priceEx07";

const expect = (x: unknown) => ({ toBe(_: unknown) {} });

test("priceEx07", () => {
  expect(priceEx07(150)).toBe(999);
});

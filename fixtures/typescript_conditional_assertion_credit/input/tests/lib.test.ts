import { price } from "../src/lib";

test("price", () => {
  const ok = true;
  if (ok) { expect(price(150)).toBe(140); }
});

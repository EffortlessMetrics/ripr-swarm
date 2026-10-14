import { capPriceVar } from "../src/capPriceVar";

test("capPriceVar", () => {
  const n = 150;
  [150, n].forEach((v) => { expect(capPriceVar(v)).toBe(140); });
});

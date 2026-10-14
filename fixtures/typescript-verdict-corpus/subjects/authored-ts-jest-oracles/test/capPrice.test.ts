import { capPrice } from "../src/capPrice";

test("capPrice", () => {
  [150, 200].forEach((v) => { expect(capPrice(v)).toBe(140); });
});

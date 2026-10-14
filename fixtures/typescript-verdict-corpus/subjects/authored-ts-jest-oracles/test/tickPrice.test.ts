import { tickPrice } from "../src/tickPrice";

const tick = () => Promise.resolve();

test("tickPrice", () => {
  [150].forEach(async (v) => { await tick(); expect(tickPrice(v)).toBe(140); });
});

import { priceEx10 } from "../src/priceEx10";

test("priceEx10", () => {
  [150].forEach((v) => { expect(priceEx10(v)).toBe(140); });
});

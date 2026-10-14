import { scaledTotal } from "../src/scaledTotal";

test("scaledTotal", () => {
  expect(scaledTotal(2, 0, 4)).toBe(2);
});

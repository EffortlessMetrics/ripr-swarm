import { priceEx15 } from "../src/priceEx15";

test("priceEx15", () => {
  expect(() => priceEx15(150)).toThrow("bad");
});

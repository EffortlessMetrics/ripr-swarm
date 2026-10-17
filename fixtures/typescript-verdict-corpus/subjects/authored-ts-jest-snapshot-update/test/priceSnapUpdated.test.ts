import { priceSnapUpdated } from "../src/priceSnapUpdated";

test("priceSnapUpdated", () => {
  expect(priceSnapUpdated(150)).toMatchSnapshot();
});

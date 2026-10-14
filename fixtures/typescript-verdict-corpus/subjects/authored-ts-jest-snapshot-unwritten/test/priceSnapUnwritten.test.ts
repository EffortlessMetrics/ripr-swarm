import { priceSnapUnwritten } from "../src/priceSnapUnwritten";

test("priceSnapUnwritten", () => {
  expect(priceSnapUnwritten(150)).toMatchSnapshot();
});

import { priceSnapSpaced } from "../src/priceSnapSpaced";

test("priceSnapSpaced", () => {
  expect(priceSnapSpaced(150)).toMatchSnapshot ();
});

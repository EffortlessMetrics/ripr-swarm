import { priceSnapConfigured } from "../src/priceSnapConfigured";

test("priceSnapConfigured", () => {
  expect(priceSnapConfigured(150)).toMatchSnapshot();
});

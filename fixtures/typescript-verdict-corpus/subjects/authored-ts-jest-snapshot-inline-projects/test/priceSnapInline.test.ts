import { priceSnapInline } from "../src/priceSnapInline";

test("priceSnapInline", () => {
  expect(priceSnapInline(150)).toMatchSnapshot();
});

import { priceSnapHint } from "../src/priceSnapHint";

test("priceSnapHint", () => {
  expect(priceSnapHint(150)).toMatchSnapshot("cap");
});

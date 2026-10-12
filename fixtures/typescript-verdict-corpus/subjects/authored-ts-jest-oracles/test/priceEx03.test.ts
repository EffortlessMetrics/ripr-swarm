import { priceEx03 } from "../src/priceEx03";

test("priceEx03", () => {
  expect(priceEx03(150)).toMatchSnapshot();
});

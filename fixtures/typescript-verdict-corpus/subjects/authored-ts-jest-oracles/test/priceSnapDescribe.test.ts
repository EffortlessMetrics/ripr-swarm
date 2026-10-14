import { priceSnapDescribe } from "../src/priceSnapDescribe";

describe("price", () => {
  test("priceSnapDescribe", () => {
    expect(priceSnapDescribe(150)).toMatchSnapshot();
  });
});

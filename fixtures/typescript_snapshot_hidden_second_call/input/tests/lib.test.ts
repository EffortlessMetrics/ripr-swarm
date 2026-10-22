import { price } from "../src/lib";

test("price", () => {
    expect(price(150)).toMatchSnapshot();
    (expect(price(90)).toMatchSnapshot)();
});

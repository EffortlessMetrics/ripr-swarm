import { summary } from "../src/summary";

test("summary of two", () => {
  expect(summary(2)).toMatchInlineSnapshot();
});

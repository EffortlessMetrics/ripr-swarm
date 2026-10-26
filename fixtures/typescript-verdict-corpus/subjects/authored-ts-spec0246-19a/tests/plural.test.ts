import { plural } from "../src/plural";

test("one item", () => {
  expect(plural(1)).toBe("1 item");
});

test("three items", () => {
  expect(plural(3)).toBe("3 items");
});

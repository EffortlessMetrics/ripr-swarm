import { plural } from "../src/plural";

test("one item", () => {
  expect(plural(1)).toBe("1 item");
});

test("three items", () => {
  expect(plural(3)).toBe("3 items");
});

test("zero items", () => {
  expect(plural(0)).toBe("0 items");
});

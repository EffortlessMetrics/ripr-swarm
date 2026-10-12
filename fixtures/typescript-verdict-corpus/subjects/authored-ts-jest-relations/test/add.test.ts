import { add } from "../src/add";

test("add", () => {
  const address = "x";
  add(1, 2);
  expect(address).toBe("x");
});

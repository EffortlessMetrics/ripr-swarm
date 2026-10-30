import { pick } from "../src/pick";

test("pick", () => {
  expect(pick(404, false)).toBe(3);
});

import { atLimit } from "../src/limit";

test("the limit flag is raised at one hundred", async () => {
  expect(Promise.resolve(atLimit(100))).resolves.toBe(true);
});

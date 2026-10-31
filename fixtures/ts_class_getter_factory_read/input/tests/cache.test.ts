import { cached } from "../src/cache";

test("cached memoizes the first result", () => {
  let runs = 0;
  const box = cached(() => ++runs);
  expect(runs).toBe(0);
  expect(box.value).toBe(1);
  expect(box.value).toBe(1);
  expect(runs).toBe(1);
});

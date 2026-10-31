import { cached } from "../src/cache";

test("cached memoizes the first result", () => {
  let runs = 0;
  let box = cached(() => ++runs);
  box = { value: 1 } as never;
  expect(runs).toBe(0);
  expect(box.value).toBe(1);
  expect(box.value).toBe(1);
});

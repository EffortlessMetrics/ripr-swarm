import { createStore } from "../src/store";

test("bump adds three", () => {
  const store = createStore(1);
  expect(store.bump()).toBe(4);
});

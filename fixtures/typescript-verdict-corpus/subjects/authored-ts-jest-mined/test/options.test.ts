import { readyOptions } from "../src/options";

test("ready options are reused, not copied", () => {
  const opts = { ready: true, retries: 2 };
  expect(readyOptions(opts)).toBe(opts);
});

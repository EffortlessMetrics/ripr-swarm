import { readyCopy } from "../src/settingsCopy";

test("ready copy keeps its fields", () => {
  const cfg = { ok: true, retries: 2 };
  expect(readyCopy(cfg).ok).toBe(true);
  expect(readyCopy(cfg)).toEqual(cfg);
});

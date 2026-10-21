import { ready } from "../src/settings";

test("ready", () => {
  const cfg = { ok: true };
  const other = { ok: true };
  expect(ready(cfg)).not.toBe(other);
  expect(ready(cfg)).toEqual(cfg);
});

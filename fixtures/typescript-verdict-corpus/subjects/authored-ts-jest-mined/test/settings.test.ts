import { readySettings } from "../src/settings";

test("ready settings come back as they are", () => {
  const cfg = { ready: true, retries: 2 };
  expect(readySettings(cfg)).toEqual(cfg);
});

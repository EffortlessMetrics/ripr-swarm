import { loadFlags, markLoaded } from "../src/settingsLoaded";

test("loaded settings come back unchanged", () => {
  const cfg = loadFlags();
  expect(markLoaded(cfg)).toEqual(cfg);
});

import { markBoth } from "../src/settingsBoth";

test("marked settings keep the input and set ok", () => {
  const cfg = { name: "bo", ok: true };
  expect(markBoth(cfg)).toEqual(cfg);
  expect(markBoth({ name: "bo" }).ok).toBe(true);
});

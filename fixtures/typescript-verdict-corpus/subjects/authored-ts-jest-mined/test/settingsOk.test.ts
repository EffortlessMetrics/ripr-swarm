import { markOk } from "../src/settingsOk";

test("marked settings equal the input", () => {
  const cfg = { name: "bo", ok: true };
  expect(markOk(cfg)).toEqual(cfg);
});

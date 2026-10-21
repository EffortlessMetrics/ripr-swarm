import { pinOk } from "../src/settingsPinned";

test("pinned settings carry ok", () => {
  expect(pinOk({ name: "bo" })).toEqual({ name: "bo", ok: true });
});

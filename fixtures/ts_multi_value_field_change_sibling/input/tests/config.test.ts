import { buildConfig } from "../src/config";

test("keeps the default timeout", () => {
  expect(buildConfig().timeout).toBe(30);
});

import { buildConfig } from "../src/config";

test("keeps the verbose flag", () => {
  expect(buildConfig().verbose).toBe(false);
});

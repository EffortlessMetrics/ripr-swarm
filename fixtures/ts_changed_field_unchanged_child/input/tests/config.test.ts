import { buildConfig } from "../src/config";

test("keeps the other retry setting", () => {
  expect(buildConfig().retries.other).toBe(1);
});

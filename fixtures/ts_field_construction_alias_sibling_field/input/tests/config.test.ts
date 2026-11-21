import { buildConfig } from "../src/config";

test("keeps the verbose flag", () => {
  const cfg = buildConfig();
  expect(cfg.verbose).toBe(false);
});

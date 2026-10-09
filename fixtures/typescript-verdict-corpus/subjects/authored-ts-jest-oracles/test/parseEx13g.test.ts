import { parseEx13g } from "../src/parseEx13g";

test("parseEx13g", () => {
  expect(() => parseEx13g("")).toThrow(globalThis.Error);
});

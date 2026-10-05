import { parseEx13t } from "../src/parseEx13t";

test("parseEx13t", () => {
  expect(() => parseEx13t("")).toThrow(TypeError);
});

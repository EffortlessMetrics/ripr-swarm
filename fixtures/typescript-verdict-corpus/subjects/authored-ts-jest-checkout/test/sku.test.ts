import { normalizeSku } from "../src/sku";

test("sku codes are trimmed and upper-cased", () => {
  expect(normalizeSku("  ab-12 ")).toEqual("AB-12");
});

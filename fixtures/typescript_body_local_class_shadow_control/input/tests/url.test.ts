import { Url } from "../src/url";

test("parses the scheme", () => {
  expect(new Url("HTTPS://x").scheme).toBe("https");
});

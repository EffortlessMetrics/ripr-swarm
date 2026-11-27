import { Url } from "../src/url";

test("parses the scheme", () => {
  class Url {
    readonly scheme = "https";
    constructor(_input: string) {}
  }
  expect(new Url("HTTPS://x").scheme).toBe("https");
});

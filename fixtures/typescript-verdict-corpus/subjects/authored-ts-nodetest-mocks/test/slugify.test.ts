import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { initials, slugify } from "../src/slugify";

describe("slugify", () => {
  it("joins words with dashes", () => {
    assert.match(slugify("Hello World"), /^hello-world$/);
  });
});

describe("initials", () => {
  it("is a short string", () => {
    assert.ok(initials("ada lovelace").length <= 2);
  });
});

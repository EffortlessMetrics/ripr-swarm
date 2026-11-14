import { describe, expect, test } from "vitest";
import { normalize } from "../src/b";

describe("normalize", () => {
  test("decodes an escaped slash", () => {
    expect(normalize("a%252Fb", (s) => s)).toBe("/a%252Fb");
  });
});

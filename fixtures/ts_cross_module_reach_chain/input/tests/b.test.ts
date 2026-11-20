import { describe, expect, test } from "vitest";
import { normalize } from "../src/b";

describe("normalize", () => {
  test("decodes an escaped slash", () => {
    expect(normalize("a%252Fb")).toBe("/ab");
  });
});

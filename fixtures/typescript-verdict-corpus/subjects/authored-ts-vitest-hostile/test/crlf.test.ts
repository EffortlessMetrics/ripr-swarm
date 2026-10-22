import { expect, it } from "vitest";
import { grade } from "../src/crlf";

it("fifty passes", () => {
  expect(grade(50)).toBe("pass");
});

it("forty-nine fails", () => {
  expect(grade(49)).toBe("fail");
});

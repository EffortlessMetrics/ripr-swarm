import { expect, test } from "vitest";
import { ParseError, parseUserClass } from "../src/parserClass";

test("an empty user is rejected with a ParseError", () => {
  expect(() => parseUserClass("")).toThrow(ParseError);
});

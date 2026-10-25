import { expect, test } from "vitest";
import { parseUserCode } from "../src/parserCode";

test("an empty user is rejected with its code", () => {
  expect(() => parseUserCode("")).toThrow({ code: "EMPTY_INPUT" });
});

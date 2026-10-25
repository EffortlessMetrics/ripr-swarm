import { expect, test } from "vitest";
import { parseUserIdent } from "../src/parserIdent";

const message = /user/;

test("an empty user is rejected", () => {
  expect(() => parseUserIdent("")).toThrow(message);
});

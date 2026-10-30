import { expect, test } from "vitest";
import { dispatch } from "../src/dispatch";

test("dispatch runs the named action once", () => {
  const calls: string[] = [];
  dispatch(
    {
      save: () => {
        calls.push("save");
      },
      load: () => {
        calls.push("load");
      },
    },
    "save",
  );
  expect(calls).toEqual(["save"]);
});

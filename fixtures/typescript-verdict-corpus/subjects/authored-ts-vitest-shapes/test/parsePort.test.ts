import { expect, it } from "vitest";
import { parsePort } from "../src/parsePort";

it("accepts a normal port", async () => {
  await expect(parsePort("8080")).resolves.toBe(8080);
});

it("rejects a port far above the range", async () => {
  await expect(parsePort("70000")).rejects.toThrow("bad port");
});

it("rejects text", async () => {
  await expect(parsePort("http")).rejects.toThrow(RangeError);
});

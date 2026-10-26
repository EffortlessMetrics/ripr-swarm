import { describe, expect, it } from "vitest";
import { bucket, ceil } from "../src/skipped";

it.skip("rounds up to the step", () => {
  expect(ceil(7, 5)).toBe(10);
});

describe.skip("buckets", () => {
  it("small below ten", () => {
    expect(bucket(3)).toBe("small");
  });
});

it("is loaded", () => {
  expect(typeof ceil).toBe("function");
});

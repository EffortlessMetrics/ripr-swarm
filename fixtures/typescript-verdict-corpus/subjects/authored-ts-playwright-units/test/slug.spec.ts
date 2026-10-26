import { expect, test } from "@playwright/test";
import { slug } from "../src/slug";

test("joins words with a dash", () => {
  expect(slug("Hello World")).toBe("hello-world");
});

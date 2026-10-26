import { test } from "node:test";
import assert from "node:assert/strict";
import { banner, fullName, greeting, initial, normalize, shout } from "../src/returns.ts";

test("greeting names the person", () => {
  assert.equal(greeting("Ada"), "Hello, Ada!");
});

test("initial table", () => {
  const rows: Array<[string, string]> = [["alice", "a"], ["", ""], ["Bob", "B"]];
  for (const [name, want] of rows) {
    assert.equal(initial(name), want);
  }
});

test("normalize is idempotent", () => {
  for (const text of ["  a ", "b", ""]) {
    assert.equal(normalize(normalize(text)), normalize(text));
  }
});

function checkShout(text: string, want: string): void {
  assert.equal(shout(text), want);
}

test("shout", () => {
  checkShout("hey", "HEY");
});

test("banner smoke", () => {
  banner("x");
});

test("full name starts with the first name", () => {
  assert.ok(fullName("Ada", "Lovelace").startsWith("Ada"));
});

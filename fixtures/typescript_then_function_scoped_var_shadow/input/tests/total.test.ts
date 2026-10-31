import { test, expect } from "vitest";
import { total } from "../src/total";
test("function scoped var shadow", () => total().then(value => { { var total = () => 30; } expect(total()).toBe(30); }));

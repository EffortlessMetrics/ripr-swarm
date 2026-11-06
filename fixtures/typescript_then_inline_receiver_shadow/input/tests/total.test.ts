import { test, expect } from "vitest";
import { total } from "../src/total";
test("inline receiver shadow", () => total().then(value => { const total = () => 30; expect(total()).toBe(30); }));

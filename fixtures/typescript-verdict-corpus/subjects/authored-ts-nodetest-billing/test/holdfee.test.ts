import test from "node:test";
import { holdFee } from "../src/holdfee";

test("hold fee adds one", (t) => {
  t["assert"].equal(holdFee(4), 5);
});

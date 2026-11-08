import { atCap } from "../src/cap";

test("the cap flag is raised at fifty", async () => {
  await expect(Promise.resolve(atCap(50))).resolves.toBe(true);
});

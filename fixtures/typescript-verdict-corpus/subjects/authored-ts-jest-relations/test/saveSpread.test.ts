import { saveSpread } from "../src/saveSpread";

const defaults = { flush: jest.fn() };

test("saveSpread", () => {
  const db = { write: jest.fn(), ...defaults };
  saveSpread(db, 1);
  expect(db.write).toHaveBeenCalledWith(2);
});

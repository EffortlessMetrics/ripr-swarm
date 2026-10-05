import { save } from "../src/save";

test("save", () => {
  const db = { write: jest.fn() };
  save(db, 1);
  expect(db.write).toHaveBeenCalledWith(2);
});

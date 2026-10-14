import { saveFactory, FactoryDb } from "../src/saveFactory";

function makeDb(): FactoryDb & { write: jest.Mock } {
  return { write: jest.fn() };
}

test("saveFactory", () => {
  const db = makeDb();
  saveFactory(db, 1);
  expect(db.write).toHaveBeenCalledWith(2);
});

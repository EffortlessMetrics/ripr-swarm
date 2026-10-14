import { buildProfileExpected } from "../src/profileExpected";

test("profile matches the expected objects", () => {
  const minor = { name: "ana", adult: false };
  const grown = { name: "bo", adult: true };
  expect(buildProfileExpected("ana", 17)).toMatchObject(minor);
  expect(buildProfileExpected("bo", 18)).toMatchObject(grown);
});

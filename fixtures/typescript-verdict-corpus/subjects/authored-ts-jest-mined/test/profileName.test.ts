import { buildProfileName } from "../src/profileName";

test("profile keeps the name", () => {
  expect(buildProfileName("bo", 18)).toMatchObject({ name: "bo" });
});

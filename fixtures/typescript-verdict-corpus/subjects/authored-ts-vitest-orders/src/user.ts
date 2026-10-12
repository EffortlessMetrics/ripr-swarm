export interface User {
  id: number;
  name: string;
}

export async function loadUser(id: number): Promise<User> {
  if (id <= 0) {
    throw new Error("user id must be positive");
  }
  return { id, name: "user" };
}

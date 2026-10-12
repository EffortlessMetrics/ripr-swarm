export function parseTitle(s: string): string {
  const title = s.trim();
  if (title === "") {
    // refuse an empty title
    throw new Error("title must not be blank");
  }
  return title;
}

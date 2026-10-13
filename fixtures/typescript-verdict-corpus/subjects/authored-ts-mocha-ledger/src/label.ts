export function parseLabel(s: string): string {
  const label = s.trim();
  if (label === "") {
    // refuse an empty label
    throw new Error("label must not be blank");
  }
  return label;
}

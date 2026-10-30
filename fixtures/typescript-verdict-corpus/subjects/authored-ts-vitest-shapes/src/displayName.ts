export interface Person {
  name?: string;
  nick?: string;
}

export function displayName(person: Person | undefined): string {
  return person?.nick ?? person?.name ?? "anonymous";
}

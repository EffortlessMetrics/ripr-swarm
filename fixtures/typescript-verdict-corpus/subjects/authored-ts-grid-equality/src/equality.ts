export function isAdmin(role: string): boolean {
  return role === "admin";
}

export function isSaturday(day: number): boolean {
  return day % 7 === 6;
}

export function samePair(a: number, b: number): boolean {
  return a === b;
}

export function isHttps(port: number): boolean {
  return port === 443;
}

export function isDefaultPort(port: number): boolean {
  return port === 8080;
}

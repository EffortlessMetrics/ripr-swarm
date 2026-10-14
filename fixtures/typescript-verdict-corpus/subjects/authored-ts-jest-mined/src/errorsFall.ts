export class NotFound extends Error {}
export class Forbidden extends Error {}

export function errorForFall(status: number): Error {
  if (status === 404) {
    return new NotFound("missing");
  }
  if (status >= 400) {
    return new NotFound("client");
  }
  return new Forbidden("denied");
}

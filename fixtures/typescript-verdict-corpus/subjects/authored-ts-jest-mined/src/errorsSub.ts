export class NotFound extends Error {}
export class Forbidden extends NotFound {}

export function errorForSub(status: number): Error {
  if (status === 404) {
    return new NotFound("missing");
  }
  return new Forbidden("denied");
}

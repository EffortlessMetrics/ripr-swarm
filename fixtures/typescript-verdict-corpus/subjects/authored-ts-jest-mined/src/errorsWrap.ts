export class NotFound extends Error {}
export class Forbidden extends Error {}

export function errorForWrap(status: number): Error {
  if (status === 404) {
    return new NotFound("missing");
  }
  return new Forbidden("denied");
}

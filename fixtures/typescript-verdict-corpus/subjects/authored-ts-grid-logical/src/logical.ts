export function canShip(paid: boolean, inStock: boolean): boolean {
  return paid && inStock;
}

export function needsReview(flagged: boolean, large: boolean): boolean {
  return flagged || large;
}

export function canEnter(hasTicket: boolean, banned: boolean): boolean {
  return hasTicket && !banned;
}

export function isVisible(hidden: boolean): boolean {
  return !hidden;
}

export function shouldRetry(timedOut: boolean, attemptsLeft: boolean): boolean {
  return timedOut && attemptsLeft;
}

/**
 * Tests a lane exists to prove, by suite and test name. The launcher turns the
 * applicable ones into RIPR_TEST_REQUIRE_RAN and the suite runner fails the
 * host run if any skipped or never ran, so a lane cannot go green on its smoke
 * tests while the journeys it names silently `this.skip()` (#10195).
 *
 * This module imports nothing: the launcher must not load mocha. A unit test
 * (required_journeys.test.ts) checks every entry against the suite sources, so
 * renaming a test cannot leave the guard requiring one that no longer exists.
 */

/** Separator for RIPR_TEST_REQUIRE_RAN; test titles contain commas. */
export const REQUIRED_TITLE_SEPARATOR = '|';

export interface RequiredJourney {
  /** Suite source file under test/suite. */
  file: string;
  suite: string;
  test: string;
}

export const PACKAGED_TRUST_TRUSTED: RequiredJourney = {
  file: 'packaged_trust.test.ts',
  suite: 'Packaged Workspace Trust',
  test: 'trusted host copies an actionable repair packet',
};

export const PACKAGED_TRUST_UNTRUSTED: RequiredJourney = {
  file: 'packaged_trust.test.ts',
  suite: 'Packaged Workspace Trust',
  test: 'untrusted host keeps an actionable repair packet out of the clipboard',
};

export const REAL_SERVER_JOURNEYS: RequiredJourney[] = [
  {
    file: 'extension.test.ts',
    suite: 'Extension Smoke',
    test: 'real server surfaces seam diagnostic, hover provider, and agent actions',
  },
  {
    file: 'extension.test.ts',
    suite: 'Extension Smoke',
    test: 'real server invalidates a seam action across deferred save and full refresh',
  },
];

export const PACKAGED_SERVER_HANDSHAKE: RequiredJourney = {
  file: 'lsp_compatibility.test.ts',
  suite: 'Standard LSP compatibility probe',
  test: 'accepts the real packaged server through initialize and shutdown',
};

export const ALL_REQUIRED_JOURNEYS: RequiredJourney[] = [
  PACKAGED_TRUST_TRUSTED,
  PACKAGED_TRUST_UNTRUSTED,
  ...REAL_SERVER_JOURNEYS,
  PACKAGED_SERVER_HANDSHAKE,
];

export function fullTitle(journey: RequiredJourney): string {
  return `${journey.suite} ${journey.test}`;
}

export function requiredJourneyTitles(
  workspaceTrustMode: string,
  testServerPath: string | undefined,
  packagedServerPath: string | undefined
): string[] {
  if (workspaceTrustMode === 'untrusted') {
    return [fullTitle(PACKAGED_TRUST_UNTRUSTED)];
  }
  const journeys = [PACKAGED_TRUST_TRUSTED];
  if (testServerPath) {
    journeys.push(...REAL_SERVER_JOURNEYS);
  }
  if (packagedServerPath) {
    journeys.push(PACKAGED_SERVER_HANDSHAKE);
  }
  return journeys.map(fullTitle);
}

<!-- section: Fixed -->
- The before-phase dirty-premise preflight no longer follows a symlinked or otherwise non-regular dirty path, so a changed file made unavailable to analysis gets the specific "analysis incomplete" refusal instead of a generic commit-first one (#11734).

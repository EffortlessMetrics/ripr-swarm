<!-- section: Internal / CI -->
- Include directive extraction during index build now runs in parallel over candidate files while preserving the same edge order, limitation order, and edge-limit cutoff as the former serial loop (#9214).

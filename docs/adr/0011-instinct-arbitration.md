# ADR 0011: Drive-modulated instinct arbitration

- Status: accepted
- Date: 2026-10-06
- Task: T18 (`instinct-core`)

## Context

The PUA → Instinct rename leaves the engine domain-agnostic: core defines how evidence becomes a
decision, while consumers define what their drives and options mean. Consumers need an optional way
to combine their existing `Scores` with drive levels and drive-to-option affinities without adding
domain concepts or a second decision gate to the engine.

Background reading on action selection includes Tinbergen (1951), Lorenz (1950), Ludlow (1976),
Redgrave, Prescott and Gurney (1999), and the Prescott, Bryson and Seth (2007) survey. These are
background reading, not verified citations.

## Decision

Add `arbitrate` to `instinct-core`. `Drives` is an ordered, non-empty set of up to 16 uniquely named
drives. A `DriveIndex` is positional. The consumer supplies drive levels and a drive-by-option
`Affinity`; core does not define drive names, levels, or meanings.

For each option, compute `pull[d][i] = D[d] * W[d][i] / 1000` using integer floor division,
`best[i] = max_d pull[d][i]`, and `urge[i] = best[i] * S[i] / 1000`, where `S` is the existing
evidence score. The lowest drive index wins a pull tie. Zero evidence therefore produces zero urge.
The raw urges are passed to the existing `decide` gate.

If an incumbent is supplied and its raw urge is at least the profile's `min_confidence`, add
`min_margin` to that option's urge with saturation at 1000. A weak incumbent receives no bonus.
`decide` remains the only gate; a near-tie freezes as `Abstain`, carrying every ranked option.
Selection is deterministic and has no stochastic component.

Consumers using `arbitrate` fold `INSTINCT_TAG` into their own `DataVersion`. Consumers that do not
use arbitration have no `DataVersion` change.

## Consequences

- Max dominance makes one drive control each option's drive pull; ties select the lowest drive index.
- Persistence models mutual inhibition with a bounded incumbent bonus. Near-ties intentionally
  freeze instead of flipping.
- The 16-drive limit bounds the table and keeps positional indices compact.
- The formula and tag must change together if arbitration semantics change.

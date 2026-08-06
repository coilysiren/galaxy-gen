# Low-mass stellar evolution

Galaxy Gen now models two deterministic post-main-sequence channels. The
existing massive-star path ends in core collapse, neutron stars, compact
mergers, and short gamma-ray bursts. Lower-mass stars instead expand into red
giants, shed planetary nebulae, and leave white dwarfs.

## Quiet channel

When a lower-mass main-sequence star reaches its mass-derived lifetime:

1. It becomes a red giant for a short resolved phase.
2. It emits a PlanetaryNebula event and returns most of its envelope to nearby
   gas with a gentle outward velocity.
3. The remaining mass and inherited heavy elements stay in a white dwarf.
4. An unpaired white dwarf eventually phase-mixes into the diffuse stellar
   halo.

Envelope loss moves inherited composition without creating baryonic mass or
new heavy elements.

## Delayed thermonuclear channel

Intermediate-mass birth draws split into close binaries using the same stable
binary ids as compact neutron-star pairs. Both components follow the quiet
channel. Once both white dwarfs reach a seed-derived delay, stellar aging emits
a TypeIaSupernova event.

The event disrupts both white dwarfs, returns their mass to local gas, emits a
causally linked shock wave, and converts up to 35% of the binary mass into new
heavy elements. Capacity-limited or fractional ejecta enter the serialized
radiated sink, keeping both ledgers closed.

## Presentation

Red giants render as large warm points. White dwarfs render as compact
blue-white points. Planetary nebulae form slowly expanding cyan and pink
shells, while Type Ia events use a brighter blue-white blast front distinct
from warm core-collapse shells.

The compact UI reports total supernovae and planetary nebulae. Debug mode
breaks that summary into live red-giant, white-dwarf, and neutron-star
populations plus separate core-collapse, Type Ia, and compact-merger counters.
Every event, stage, binary id, age, delay, and counter survives the existing
worker state round-trip.

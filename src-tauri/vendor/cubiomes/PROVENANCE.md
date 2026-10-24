# Vendored cubiomes

Source: https://github.com/xpple/cubiomes
Commit: `d934de1d4e5e71db0b789296241f929a50306504` (2026-10-23, master)
License: MIT (see `LICENSE`)

Only the core generator, biome/feature tables, terrain noise, and the
`features/` structure helpers are vendored. The upstream loot table code is not
included because the seed-map tool does not read structure loot.

Local changes: none to upstream files; `elytra_shim.*` is Elytra-specific.

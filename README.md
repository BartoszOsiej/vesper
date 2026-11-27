# VESPER

A space sandbox you can actually launch: a ship from our own MTS-Ships generator,
flight, mining, a station and trading. Custom renderer (wgpu/Vulkan), no code from
NV2_ENGINE.

## Running

```bash
cargo run --release
```

Requires a GPU with Vulkan/GL 3.3. Tested on a GeForce 940MX (2 GB) — holds 60 fps.

`VESPER_START_DOCKED=1 cargo run` opens the station panel immediately (handy for UI work).

## Controls

| key | action |
|---|---|
| W / S | pitch |
| A / D | yaw |
| Q / E | roll |
| ↑ / ↓ | throttle 0–900 m/s |
| Shift | afterburner (×4) |
| F | mine the targeted asteroid |
| Space | fire at the targeted pirate |
| G | dock / undock (station within 900 m) |
| H | jump to the next sector (costs 1 fuel) |
| 1 / 2 / 3 | while docked: sell ore / buy fuel / sell alloy |
| Shift + 1 | while docked: buy ore (5 units for 12 cr) |
| 5 | while docked: refine 10 ore into 8 alloy |
| 4 | while docked: claim the bounty contract |
| F5 / F9 | save / load `savegame.txt` |
| F11 | fullscreen |
| Esc | quit |

## Layout

- `src/main.rs` — game state, flight, combat, mining, refinery, local prices,
  two sectors, contracts, saves, HUD
- `src/gfx.rs` — wgpu renderer: HDR scene (`Rgba16Float`) with 4× MSAA, composite
  pass (bloom, ACES, grading, vignette), instanced mesh pipeline with triplanar
  detail, additive pass for engines/tracers/explosions/stars, and the 2D UI pipeline
- `src/post.wgsl` — frame compositor: 16-tap bloom, ACES curve, color grading
- `src/textures.rs` — procedural hull and rock textures (albedo/height/roughness),
  generated at startup, no image files
- `src/mesh.rs` — procedural frigate (lofted hull, delta wings, engine nacelles),
  icosahedron asteroids (2 subdivisions, fBm and ridges), station geometry
  (torus rings, spokes, docking arms, panels); OBJ loader kept for assets
- `src/ui.rs` — 3×5 micro-font, text drawn as pixels (no atlas)
- `src/market.rs` — P2S market: parser, SHA-256 handshake
- `assets/ships/frigate.obj` — reference asset from the MTS-Ships generator
  (`midpoly/ships.py`), verified by the market handshake
- `assets/market/market.txt` — local market copy + asset manifest

## Market P2S — handshake

The client starts with a local market copy and an asset hash manifest. On startup it
computes SHA-256 for every asset and compares it with the manifest. **A mismatch stops
the game:**

```
market rejected: asset ships/frigate.obj does not match the manifest
(expected e0a7326…, got e6326c8…)
```

That guards against a swapped model and against a client from another build.

## Gameplay loop

mine ore → refine (10 ore → 8 alloy) → fight pirates (+350 cr per kill, contract for
3 pirates worth 900 cr) → dock → sell ore and alloy / buy fuel → jump to the next
sector. Alloy is worth 26 cr per unit, ore only 8 cr, so refining pays off. Pirates
respawn, so a sector never goes quiet. Death respawns you at the station and wipes cargo.

Cargo holds 200 m³: ore takes 1 m³, alloy 0.6 m³. Mining stops when there is no room.

There are two sectors (HARTWELL and KEPLER-9) with separate asteroid fields, pirates and
station colors. Prices are local: HARTWELL is the industrial hub (fuel 45 cr, alloy 22 cr),
KEPLER-9 is the frontier (fuel 70 cr, alloy 32 cr). Refining in HARTWELL and hauling alloy
to KEPLER-9 pays, and fuel is cheapest where it is produced.

A jump costs 1 fuel; game state (sector, credits, both cargo holds, contract, position)
can be saved with F5 and loaded with F9.

Flight is physical: asteroids and the station have collision spheres, an impact damages
the hull proportionally to speed and bounces the ship off. The station is 264 m in radius
and visible from far away — docking is a slow orbit around the ring.

## What is missing

Animation, sound, P2P networking, more than two sectors, production beyond the refinery,
and missions other than bounties. Lighting is a shadow map (2048², PCF 3×3) plus key,
fill and ambient, with screen-space bump from the procedural height; the nebula is a
background shader rather than volumetric geometry.

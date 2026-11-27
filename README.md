# VESPER

Nasze EVE w wersji, którą da się uruchomić: statek z własnego generatora MTS-Ships,
lot, wydobycie, stacja i handel. Renderer własny (wgpu/Vulkan), zero kodu z NV2_ENGINE.

## Uruchomienie

```bash
cargo run --release
```

Wymaga karty z Vulkanem/GL 3.3. Testowane na GeForce 940MX (2 GB) — trzyma 60 fps.

`VESPER_START_DOCKED=1 cargo run` otwiera od razu panel stacji (przydatne do testów UI).

## Sterowanie

| klawisz | akcja |
|---|---|
| W / S | pochylenie (pitch) |
| A / D | obrót (yaw) |
| Q / E | przechylenie (roll) |
| ↑ / ↓ | ciąg 0–900 m/s |
| Shift | dopalacz (×4) |
| F | wydobycie z namierzonej asteroidy |
| Spacja | ogień do namierzonego pirata |
| G | dokowanie / odczepienie (stacja musi być w zasięgu 700 m) |
| 1 / 2 / 3 | w dokowaniu: sprzedaj rudę / kup paliwo / kup stop |
| Shift + 1/2/3 | w dokowaniu: kup dany towar |
| Esc | wyjście |

## Struktura

- `src/main.rs` — stan gry, lot, wydobycie, dokowanie, HUD
- `src/gfx.rs` — renderer wgpu: pipeline siatek z instancingiem + pipeline UI 2D
- `src/mesh.rs` — loader OBJ (płaskie normalne), asteroidy z icosahedronu, geometria stacji
- `src/ui.rs` — mikro-font 3×5 i rysowanie tekstu jako pikseli (bez atlasów)
- `src/market.rs` — market P2S: parser, hash SHA-256, handshake
- `assets/ships/frigate.obj` — statek z generatora MTS-Ships (`talus/midpoly/ships.py`)
- `assets/market/market.txt` — lokalna kopia rynku + manifest assetów

## Market P2S — handshake

Klient startuje z lokalną kopią rynku i manifestem hashów. Przy starcie liczy SHA-256
każdego assetu i porównuje z manifestem. **Niezgodność = gra nie startuje:**

```
rynek odrzucony: asset ships/frigate.obj niezgodny z manifestem
(oczekiwano e0a7326…, jest e6326c8…)
```

To jest zabezpieczenie przed podmienionym modelem i przed klientem z innego builda.

## Pętla rozgrywki

wydobycie rudy → walka z piratami (6 w sektorze, +350 kr za zestrzelenie) →
dokowanie → sprzedaż rudy / zakup paliwa. Śmierć = odrodzenie przy stacji i utrata rudy.

## Czego nie ma

Tekstur (jest tylko materiał z cieniowania), animacji, dźwięku, sieci P2P,
więcej niż jednego sektora, misji i przemysłu.

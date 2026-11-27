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
| G | dokowanie / odczepienie (stacja musi być w zasięgu 900 m) |
| H | skok do następnego sektora (kosztuje 1 paliwo) |
| 1 / 2 / 3 | w dokowaniu: sprzedaj rudę / kup paliwo / sprzedaj stop |
| Shift + 1 | w dokowaniu: kup rudę (5 szt. za 12 kr) |
| 5 | w dokowaniu: rafinuj 10 rudy na 8 stopu |
| 4 | w dokowaniu: odbierz nagrodę za kontrakt |
| F5 / F9 | zapis / odczyt `savegame.txt` |
| F11 | pełny ekran |
| Esc | wyjście |

## Struktura

- `src/main.rs` — stan gry, lot, walka, wydobycie, rafineria, lokalne ceny,
  dwa sektory, kontrakty, zapis, HUD
- `src/gfx.rs` — renderer wgpu: scena HDR (`Rgba16Float`) z MSAA 4×, pass kompozytu
  (bloom, ACES, grading, winieta), pipeline siatek z triplanarem, addytywny pass emisji
  (silniki, tracery, wybuchy, gwiazdy) i pipeline UI 2D
- `src/post.wgsl` — kompozytor klatki: 16 próbek bloomu, krzywa ACES, korekcja kolorów
- `src/textures.rs` — proceduralne tekstury poszycia i skały (albedo/wysokość/szorstkość),
  bez plików, generowane przy starcie
- `src/mesh.rs` — loader OBJ (płaskie normalne), asteroidy z icosahedronu (2× podział,
  fBm i wypukłości), geometria stacji (torusy, szprychy, ramiona dokowania, panele)
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

wydobycie rudy → rafinacja (10 rudy → 8 stopu) → walka z piratami
(+350 kr za zestrzelenie, kontrakt na 3 piratów za 900 kr) → dokowanie →
sprzedaż rudy i stopu / zakup paliwa → skok do kolejnego sektora.
Stop jest wart 26 kr za sztukę, ruda tylko 8 kr, więc opłaca się ją przetworzyć.
Piraci wracają, więc sektor nie pustoszeje. Śmierć = odrodzenie przy stacji i utrata ładowni.

Ładownia ma 200 m³: ruda zajmuje 1 m³, stop 0,6 m³. Wydobycie zatrzymuje się,
gdy brakuje miejsca.

Są dwa sektory (HARTWELL i KEPLER-9) z osobnym układem asteroid, piratów i kolorem
stacji. Ceny są lokalne: HARTWELL to hub przemysłowy (paliwo 45 kr, stop 22 kr),
KEPLER-9 to frontier (paliwo 70 kr, stop 32 kr). Opłaca się rafinować w HARTWELL
i wozić stop do KEPLER-9, a paliwo tankować tam, gdzie jest tanie.

Skok zabiera 1 paliwo; stan gry (sektor, kredyty, obie ładownie, kontrakt, pozycja)
można zapisać pod F5 i wczytać pod F9.

Lot jest fizyczny: asteroidy i stacja mają kolizje (sfery), uderzenie uszkadza kadłub
proporcjonalnie do prędkości, a statek odbija się od przeszkody. Stacja ma 264 m
promienia i widać ją z daleka — dokowanie to powolny oblot wokół pierścienia.

## Czego nie ma

Animacji, dźwięku, sieci P2P, więcej niż dwóch sektorów, produkcji poza rafinerią
i misji innych niż bounty. Cienie są nadal tylko ekranowe (brak shadow map),
a normalne pochodzą z geometrii i proceduralnej wysokości, nie z tekstur normalnych.

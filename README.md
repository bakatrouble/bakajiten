# bakajiten — 0.1.1

Japanese dictionary lookup with handwriting recognition — a Qt
Widgets desktop app on a Rust backend.

Type a word, draw a kanji, or search by English meaning. Results rank
by frequency; conjugated forms deinflect to their
dictionary entries; hovering any kanji shows its readings at a glance
and double-click opens the full information dialog.

## Features

- **Tiered search** — exact, prefix, suffix, and substring modes over
  Japanese headwords and readings, plus English-meaning search and a
  names scope (JMnedict) alongside the monolingual kokugo corpus
- **Deinflection** — conjugated verbs and adjectives resolve to their
  dictionary forms with the reason chain shown (~375 rules)
- **IME-style kana input** — romaji converts live as you type
- **Handwriting recognition** — draw on the canvas; classical
  trajectory matcher (Sólyom Zoltán's zkanji algorithm) ranks
  candidates in milliseconds, no ML model
- **Kanji deep-dives** — hover cards everywhere, stroke-order diagrams
  with playback, parts/similar/part-of browsers, JLPT and grade data
- **Offline data** — memory-mapped artifacts built from open sources;
  virtualized lists stay smooth at any result count

## Download

Prebuilt Linux (AppImage) and Windows binaries are published with
each release — no build needed.

## Building from source

Build the dictionary data first (one time, ~10 min — bulk sources
download into `dataimports/`, packed artifacts land in `data/`):

```sh
cargo run -p utils --bin fetch-data -- --out-dir dataimports
cargo run --release -p utils --bin fetch-data -- --kokugo-out dataimports/kokugo/kokugo.jsonl --fetch
cargo run --release -p utils --bin pack-data -- --kanjivg dataimports/kanjivg \
    --templates-out data/templates.bin \
    --kanjidic dataimports/kanjidic2.xml --kanji-out data/kanji.bin \
    --jmdict dataimports/JMdict.xml --jmdict-out data/jmdict.bin \
    --jmnedict dataimports/JMnedict.xml --jmnedict-out data/jmnedict.bin \
    --kokugo dataimports/kokugo/kokugo.jsonl --kokugo-out data/kokugo.bin
```

Desktop app (Qt6, C++26, `uniffi-bindgen-cpp` on PATH, system yaml-cpp;
clone with submodules so `frontends/qt` is present):

```sh
git clone --recurse-submodules git@github.com:bakatrouble/bakajiten.git
cmake -S frontends/qt -B frontends/qt/build
cmake --build frontends/qt/build
cd frontends/qt && ./build/bakajiten
```

Tests: `ctest` in a configured Qt dir (offscreen) and `cargo test`
for Rust.

## Data sources

| Source | Used for | License |
|---|---|---|
| [JMdict / JMnedict / KANJIDIC2](http://www.edrdg.org/) (EDRDG) | word, name, and kanji entries | CC-BY-SA |
| [Japanese Wiktionary](https://kaikki.org/jawiktionary/rawdata.html) via kaikki | kokugo definitions | CC-BY-SA 4.0 |
| [Japanese WordNet](https://bond-lab.github.io/wnja/) | kokugo backfill + relations | CC-BY 4.0 |
| [KanjiVG](https://kanjivg.tagaini.net/) | stroke data + recognition templates | CC-BY-SA 3.0 |
| JLPT levels (Jonathan Waller / Tanos) | word levels | CC-BY |

Recognition, inflection handling, ranking — and the interface itself —
follow [zkanji](https://github.com/z1dev/zkanji) by Sólyom Zoltán.

## Project structure

```text
librecognizer/   classical handwriting matcher (trajectories, DTW, layout, rank)
libdict/         dictionary storage + lookup, UniFFI boundary
utils/           data pipeline: fetch-data (incl. kokugo import), pack-data
frontends/qt/     Qt Widgets desktop app (bakajiten-qt submodule)
dataimports/     bulk sources (not in git)
data/            packed artifacts (not in git)
```

## Contributing

Issues and pull requests welcome. Please run the relevant suite
before submitting (`ctest` or `cargo test`) and keep
translation units small and single-purpose.

## License

MIT — see [LICENSE](LICENSE).

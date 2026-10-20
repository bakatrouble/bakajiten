# AGENTS.md — working notes for this repo (untracked, do not publish)

## Session rules

- Short, factual replies. No emojis unless asked. No superlatives.
- Never generate/guess URLs; use only ones from the user or local files.
- Verify through execution: build + run tests after changes. Evidence
  before synthesis — read files, don't trust memory.
- No visual testing unless the user explicitly overrides for one
  validation. GUI tests run on Xvfb `:99`
  (`env -u WAYLAND_DISPLAY DISPLAY=:99`); Qt needs
  `QT_QPA_PLATFORM=xcb`; QTest uses `QT_QPA_PLATFORM=offscreen`.
- Kill test apps by exact PID (`kill -9 <pid>`); never `pkill -f`.
- No live testing unless QTest and logging options are exhausted.
- `qInfo` is silent under redirected stderr; use
  `QT_FORCE_STDERR_LOGGING=1`.
- `build/`, `build-dev/`, `/tmp/opencode` are wipeable. Background
  shells notify; never poll.
- `QTest::qWaitForWindowExposed` + `height() > 0` guards;
  `QTest::keyClicks` is ASCII-only; one test-function arg per QTest
  invocation. WM-less Xvfb `hasFocus` fails; offscreen `mapToGlobal`
  runs 2px off; `QTest::mouseMove` leaves phantom moves (hand-build
  `QMouseEvent`s instead).
- `QSettings` YAML backend is shared suite state — tests must clean up.
- App Qt target is warnings-as-errors (test target is not); always
  build the app target before finishing.
- `uniffi-bindgen-cpp` emits `k`-prefixed 1-based enums and needs
  unstripped cdylibs; verify builds by exit code.
- `npm test` needs `npm run build` first (e2e + synced vendor copies).
- Watch for concurrent sessions: Designer `.ui` churn, selective
  reverts, checkpoint commits sweeping in-flight work — verify diffs
  before committing, never `git checkout`/`stash` shared files.
- Remember active user corrections across turns until explicitly lifted.

## Code conventions

- PascalCase file names, lowercase directories; ~200-line soft cap per
  TU, one responsibility per file.
- No `bakajiten` prefix on helper/API names. No `#[path]` attributes
  (Rust). No `clang-format` while iterating.
- Qt6 + CMake + UniFFI C++ bindings; workspace pins `uniffi = "0.31"`.
- Storage: plain `repr(u8)` enums + byte indices, never bitmasks/heap
  strings; borrow-only conversions; unknown entities fail pack loudly.
- Scale slider 25% steps only; font sizes follow UI scale everywhere.
- Toolbar buttons stay square (40px floor at 100%, absolute, never
  scaled). Panel size persists in the separate YAML state file.
- Mode switch (Japanese/meaning) clears the search field; scope combo
  is English/Names/Kokugo, exactly one dictionary loaded at a time.
- No archive validation: header check (magic, version, exact length)
  is the only runtime gate; pack outputs roundtrip-verify at write time.
- kokugo import: audited (every skip counted), deterministic,
  self-validating; never fabricate readings or verb classes.
- Single global tooltip show delay (200ms); hover on kanji anywhere
  shows the tooltip; double-click opens the modeless kanji dialog.
- Deinflection reason names copy zkanji `wordInflection` vocabulary.
- IME kana conversion: backspace deletes the last display character
  whole (never un-converts); partial mora is modal (only backspace
  and conversion keystrokes land).

## Layout

- `lib/librecognizer/` — geometry, classical matcher, recognition model.
- `lib/libdict/` — dictionary storage + lookup (UniFFI).
- `utils/` — data pipeline (`pack-data`, `fetch-data` incl. kokugo
  import, `src/parsers`).
- `frontends/qt/` — Qt Widgets desktop app (`bakajiten-qt` submodule;
  its own repo owns Qt CI + releases, tags are cut there).
- `frontends/web/` — Vue 3 SPA over the same backend (WASM;
  `bakajiten-web` submodule, deploys itself to project pages).
- `dataimports/` — bulk sources (gitignored); `data/` — packed
  artifacts (gitignored). Neither ships in git.

## Data pipeline (was: root README + utils notes)

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

- JLPT levels come from vendored `dataimports/jlpt.csv` (tanos.co.uk,
  CC-BY); pass via `pack-data --jlpt`.
- Each pack output is independent and roundtrip-verified at write time.
- Sources/licenses: EDRDG (JMdict/JMnedict/kanjidic2/radkfile),
  KanjiVG, kradfile, kaikki jawiktionary (CC-BY-SA 4.0) + Japanese
  WordNet (CC-BY 4.0) for kokugo.
- Bindings regen: C++ at every Qt build (UniFFI 0.31 lockstep);
  TypeScript via `uniffi-bindgen-js`, Kotlin via pinned throwaway
  driver. FFI error payloads stay named fields (tuple variants emit
  invalid TS). `from_paths` is native-only; WASM uses byte ctors.
- `uniffi` workspace dep carries `scaffolding-ffi-buffer-fns` +
  `wasm-unstable-single-threaded`.

## Recognizer (was: architecture.md)

Sólyom Zoltán's classical two-stage matcher (zkanji): per-stroke
segment-angle DTW + 16-bit relative-position layout, fused per
candidate. Everything trajectory-native, no images.

- Pen space 0..255 (u8); matcher divides into unit space. KanjiVG
  sources are 109-native, scaled at parse.
- Pipeline: `simplify` (denoise: `TINY_MOUSE 0.00725`,
  `TINY_MODEL 0.00522`) → `dissect` (181 resample points, corner
  commit at `dsum > len·LENMUL`) → shape DTW (cost `|Δangle|·800`,
  hook suppression ends `< min(0.2, len·0.18)`, norm `/steps·10`) +
  layout (`compute_positions` bands, `pos_diff` squared gaps).
- Fusion per candidate: greedy stroke assignment (order window ±1,
  unused-only, cap 100000) + layout vs ±2 neighbors + ±5% size prior
  + missing-stroke fines. Lower wins; absolute values meaningless.
- Bank: `build_templates` deterministic; rayon map + sequential sort
  ⇒ bit-identical at any thread count. Full-bank match ~8 ms.
- Artifacts: `KTM1`/`KTT1` templates, `KJD1` kanji records.

## Deinflection (was: deinflection-rules.md)

382 rules, BFS over surfaces (visited-set on surface, depth cap 5),
states union class claims; final exact lookup needs
`entry.classes ∩ state ≠ ∅`. Reason names copy zkanji
`wordInflection`. Rules bucketed by final kana of `from`
(longest-first); compiler asserts one-bucket-per-rule.

- Classes from JMdict pos: `1` (v1), `5k…5u` godan rows, `5ks` (iku),
  `5aru`, `k` (vk), `s` (vs), `i` (adj-i), `z` (vz). `v4/v2` compile
  with no rules (reserved).
- Families: polite (50), negative (32), past/te (34),
  conditionals/concessives (32), voice (44), volitional/imperative
  (43), desire/evaluative (94), contractions/te-compounds (41, all
  transparent), copula tails (4).
- Deliberately out: prefix honorifics, compound-verb boundaries,
  particles, derivational suffixes, dialects, classical rules,
  empty-surface candidates (engine must reject).
- Worked acceptance chains live in the Rust tests (depth ≤ 4 cases).

## Dictionary engine (was: dictionary-library.md)

- One normalizer everywhere (NFC, katakana→hiragana fold for keys,
  full→half-width; no dakuten folding).
- Artifacts: versioned envelope + string pool + flat tables +
  trigram postings; `forms`/`forms_rev` sorted tables searched by
  `partition_point`; gloss trigram namespace separate.
- mmap + lazy decode; pre-ranked rows; short queries fall back to
  bounded scans; unigram tier serves 1-char hits rank-capped.
- Ranking: match class (exact+deinflected share a frequency bucket),
  then prefix > suffix; `is_common` first; `ent_seq` tiebreak.
- Public surface: `open`, `lookup_exact/prefix/suffix`,
  `search_contains/gloss`, `deinflect`, merged `search`, `entry_by_seq`.
- Budgets: exact < 100 µs, affix < 1 ms, contains < 10 ms,
  deinflect+lookup < 2 ms, cold open < 200 ms.

## Query-edge conversion (was: romaji-kana.md + ime-conversion.md)

- Batch: lowercase, whole-word exceptions (konnichiwa/konbanwa),
  longest-token-first (~120 tokens, Hepburn + Kunrei accepted),
  `n`-lookahead, geminate lookahead (`k s t p c f h b d g z j v`,
  plus `mm`→ん), fallthrough raw. Total, infallible. Always
  hiragana out; long vowels never merged.
- Incremental: single `raw` string; `display = batch(head,
  lookahead=tail[0]) + tail` over the longest live-tail split.
  Backspace deletes whole display chars and resyncs; pending mora is
  UI-modal. Acceptance traces live in the Rust tests.

## Dictionary UI data (was: gloss-examples.md, pitch-accent.md)

- Gloss showcase entries (1596430, 2241040, 1512710, 1336340, 2851104,
  2028990, 1352320) exercise reading/headword attribution, `stagk`,
  multilingual glosses, loanword sources, xrefs — look up by headword
  to test the article renderer.
- Pitch accent: research only, nothing packed. Verdicts: NHK no
  (commercial), Kanjium maybe (license unverified), UniDic usable
  (morpheme-level, sandhi open), Kaikki/Wadoku/OJAD no.

//! Pack all runtime data files from source checkouts.
//!
//! ```sh
//! cargo run --release -p utils --bin pack-data -- --kanjivg dataimports/kanjivg \
//!     --templates-out data/templates.bin \
//!     --kanjidic dataimports/kanjidic2.xml --kanji-out data/kanji.bin \
//!     --jmdict dataimports/JMdict.xml --jmdict-out data/jmdict.bin \
//!     --jmnedict dataimports/JMnedict.xml --jmnedict-out data/jmnedict.bin
//! ```
//!
//! Each output is independent: pass only the flags for the files you
//! want. Every output is verified with a decode roundtrip before
//! writing.

use clap::Parser;
use std::path::PathBuf;

mod convert;
mod gloss;
mod jlpt;
mod kanji;
mod kokugo;
mod progress;
mod templates;

#[derive(Parser)]
#[command(
    name = "pack-data",
    about = "Pack KanjiVG + EDRDG sources into runtime data files",
    version
)]
struct Args {
    /// KanjiVG directory (*.svg) or single kanjivg.xml dump
    #[arg(long)]
    kanjivg: Option<PathBuf>,
    /// Precomputed template-bank file (the app loads this)
    #[arg(long)]
    templates_out: Option<PathBuf>,
    /// kanjidic2 XML (joins with KanjiVG for `--kanji-out`)
    #[arg(long)]
    kanjidic: Option<PathBuf>,
    /// kradfile parts index (joins with kanjidic2 for `--kanji-out`;
    /// absent entries pack with empty parts)
    #[arg(long)]
    kradfile: Option<PathBuf>,
    /// kanjidic2 + strokes record file
    #[arg(long)]
    kanji_out: Option<PathBuf>,
    /// JMdict XML source
    #[arg(long)]
    jmdict: Option<PathBuf>,
    /// Word-dictionary record file
    #[arg(long)]
    jmdict_out: Option<PathBuf>,
    /// JMnedict XML source
    #[arg(long)]
    jmnedict: Option<PathBuf>,
    /// Name-dictionary record file
    #[arg(long)]
    jmnedict_out: Option<PathBuf>,
    /// Kokugo source: kaikki extract (plain JSONL, unpacked by
    /// fetch-data)
    #[arg(long)]
    kokugo: Option<PathBuf>,
    /// WordNet LMF (joins the kokugo import)
    #[arg(long)]
    wordnet: Option<PathBuf>,
    /// JMdict XML for kokugo reading backfill (reb-less entries take
    /// the JMdict reading when exactly one exists across all keb)
    #[arg(long)]
    jmdict_backfill: Option<PathBuf>,
    /// Monolingual-dictionary record file
    #[arg(long)]
    kokugo_out: Option<PathBuf>,
    /// JLPT word-level snapshot (`dataimports/jlpt.csv`): optional,
    /// missing entries pack as unknown (0)
    #[arg(long)]
    jlpt: Option<PathBuf>,
    /// Print a JSON summary of the packed artifacts instead of the
    /// human-readable lines (same versions either way)
    #[arg(long)]
    json: bool,
}

/// One packed artifact: returned by every pack step, printed by
/// main (human lines or `--json`).
#[derive(serde::Serialize)]
struct Summary {
    artifact: String,
    path: PathBuf,
    records: usize,
    bytes: usize,
    version: String,
}

fn print_summary(summary: &Summary, json: bool) {
    if json {
        println!(
            "{}",
            serde_json::to_string(summary).expect("summary serializes")
        );
    } else {
        println!(
            "saved {} to {} ({} records, {} bytes, {:.1} bytes/record) [version {}]",
            summary.artifact,
            summary.path.display(),
            summary.records,
            summary.bytes,
            summary.bytes as f64 / summary.records.max(1) as f64,
            summary.version,
        );
    }
}

fn packed_at() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// KanjiVG source version (`version.txt` in the checkout, else
/// "unknown": checkouts carry no other marker).
fn kanjivg_version(dir: &std::path::Path) -> String {
    std::fs::read_to_string(dir.join("version.txt"))
        .map(|s| s.trim().to_owned())
        .unwrap_or_else(|_| "unknown".into())
}

/// File count for the load bar: `.svg` files in a directory, else 1
/// (single-file input parses in one step).
fn kanjivg_file_count(path: &std::path::Path) -> u64 {
    if !path.is_dir() {
        return 1;
    }
    std::fs::read_dir(path)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("svg"))
                .count() as u64
        })
        .unwrap_or(0)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    if args.templates_out.is_some() && args.kanjivg.is_none() {
        return Err("--templates-out needs --kanjivg".into());
    }
    if args.kanji_out.is_some() && (args.kanjivg.is_none() || args.kanjidic.is_none()) {
        return Err("--kanji-out needs --kanjivg and --kanjidic".into());
    }
    if args.jmdict_out.is_some() && args.jmdict.is_none() {
        return Err("--jmdict-out needs --jmdict".into());
    }
    if args.jmnedict_out.is_some() && args.jmnedict.is_none() {
        return Err("--jmnedict-out needs --jmnedict".into());
    }
    if args.kokugo_out.is_some() && (args.kokugo.is_none() || args.wordnet.is_none()) {
        return Err("--kokugo-out needs --kokugo and --wordnet".into());
    }

    // KanjiVG parses once, shared by both stroke outputs.
    let parsed = match &args.kanjivg {
        Some(dir) if args.templates_out.is_some() || args.kanji_out.is_some() => {
            let total = kanjivg_file_count(dir);
            let bar = progress::count_bar("load kanjivg", total);
            let mut n = 0u64;
            let entries = utils::parsers::kanjivg::load_path_with(dir, &mut || {
                n += 1;
                bar.inc(1);
            })?;
            bar.finish_with_message(format!("load kanjivg ({n} files)"));
            Some(entries)
        }
        _ => None,
    };
    if let (Some(entries), Some(out)) = (&parsed, &args.templates_out) {
        let dir = args.kanjivg.as_deref().expect("checked above");
        let summary = templates::pack(entries, out, packed_at(), kanjivg_version(dir))?;
        print_summary(&summary, args.json);
    }
    if let (Some(entries), Some(kanjidic), Some(out)) = (&parsed, &args.kanjidic, &args.kanji_out) {
        let kanjivg_dir = args.kanjivg.as_deref().expect("checked above");
        let summary = kanji::pack(
            entries,
            kanjidic,
            args.kradfile.as_deref(),
            kanjivg_dir,
            out,
        )?;
        print_summary(&summary, args.json);
    }
    if let (Some(xml), Some(out)) = (&args.jmdict, &args.jmdict_out) {
        let jlpt = load_jlpt(&args.jlpt)?;
        let summary = gloss::pack_jmdict(xml, out, &jlpt)?;
        print_summary(&summary, args.json);
    }
    if let (Some(xml), Some(out)) = (&args.jmnedict, &args.jmnedict_out) {
        let jlpt = load_jlpt(&args.jlpt)?;
        let summary = gloss::pack_jmnedict(xml, out, &jlpt)?;
        print_summary(&summary, args.json);
    }
    if let (Some(kaikki), Some(wordnet), Some(out)) = (&args.kokugo, &args.wordnet, &args.kokugo_out) {
        let jlpt = load_jlpt(&args.jlpt)?;
        let summary = gloss::pack_kokugo(kaikki, wordnet, out, &jlpt, args.jmdict_backfill.as_deref())?;
        print_summary(&summary, args.json);
    }
    Ok(())
}

/// JLPT index for the gloss packs (empty when `--jlpt` is absent —
/// every record packs as unknown).
fn load_jlpt(path: &Option<PathBuf>) -> Result<jlpt::JlptIndex, Box<dyn std::error::Error>> {
    match path {
        Some(p) => jlpt::JlptIndex::load(p),
        None => Ok(jlpt::JlptIndex::default()),
    }
}

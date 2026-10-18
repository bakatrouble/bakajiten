//! Download dictionary + KanjiVG sources with terminal progress.
//!
//! ```sh
//! cargo run -p utils --bin fetch-data [--out-dir dataimports] [--kanjivg-dir dataimports/kanjivg] [--only jmdict,kanjivg]
//! ```
//!
//! Fetches the four canonical EDRDG files (gunzipped on the fly),
//! the latest KanjiVG `*-all.zip` release (unpacked to per-glyph
//! SVGs), and the kokugo sources (Kaikki extract, unpacked to plain
//! JSONL; WordNet LMF as-is):
//!
//! ```text
//! key        remote                     -> local
//! jmdict     JMdict.gz                  -> <out-dir>/JMdict.xml
//! jmnedict   JMnedict.xml.gz            -> <out-dir>/JMnedict.xml
//! kanjidic   kanjidic2.xml.gz           -> <out-dir>/kanjidic2.xml
//! radkfile   radkfile.gz                -> <out-dir>/radkfile
//! kanjivg    KanjiVG latest *-all.zip   -> <out-dir>/kanjivg/*.svg
//! kokugo     kaikki extract (.gz)       -> <out-dir>/ja-extract.jsonl
//!            WordNet LMF                -> <out-dir>/wn-ntumc-jpn.xml
//! ```
//!
//! `--only` selects a comma-separated subset (default: everything).
//! Bulk datasets re-download every run, so local iteration should
//! pass `--only` explicitly.
//! Each download gets its own progress bar (bytes, throughput, ETA).
//! HTTP errors fail the run; corrupt payloads fail the decode instead
//! of writing partial files silently. Existing files are overwritten
//! (the KanjiVG zip itself is removed after unpacking).

use clap::Parser;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use ureq::ResponseExt as _;

/// One fetchable source file: selection key, remote URL, and the
/// local filename (`.gz` URLs decode on the fly, anything else
/// downloads raw).
struct Source {
    key: &'static str,
    url: &'static str,
    file: &'static str,
}

const SOURCES: &[Source] = &[
    Source {
        key: "jmdict",
        url: "http://ftp.edrdg.org/pub/Nihongo/JMdict.gz",
        file: "JMdict.xml",
    },
    Source {
        key: "jmnedict",
        url: "http://ftp.edrdg.org/pub/Nihongo/JMnedict.xml.gz",
        file: "JMnedict.xml",
    },
    Source {
        key: "kanjidic",
        url: "https://www.edrdg.org/kanjidic/kanjidic2.xml.gz",
        file: "kanjidic2.xml",
    },
    Source {
        key: "radkfile",
        url: "http://ftp.edrdg.org/pub/Nihongo/radkfile.gz",
        file: "radkfile",
    },
    Source {
        key: "kradfile",
        url: "http://ftp.edrdg.org/pub/Nihongo/kradfile.gz",
        file: "kradfile",
    },
    Source {
        key: "kokugo",
        url: "https://kaikki.org/jawiktionary/raw-wiktextract-data.jsonl.gz",
        file: "ja-extract.jsonl",
    },
    Source {
        key: "kokugo",
        url: "https://raw.githubusercontent.com/bond-lab/wnja/master/data/wn-ntumc-jpn.xml",
        file: "wn-ntumc-jpn.xml",
    },
];

/// All fetchable datasets, in display order.
const KEYS: &[&str] = &[
    "jmdict", "jmnedict", "kanjidic", "radkfile", "kradfile", "kanjivg", "kokugo",
];

const BAR_TEMPLATE: &str =
    "{msg:12} {bar:40.cyan/blue} {bytes}/{total_bytes} {bytes_per_sec} eta {eta}";

#[derive(Parser)]
#[command(
    name = "fetch-data",
    about = "Download + unpack dictionary and KanjiVG sources",
    version
)]
struct Args {
    /// Output directory for the decompressed dictionary files (created if missing)
    #[arg(long, default_value = "dataimports")]
    out_dir: PathBuf,
    /// Only fetch these datasets (comma-separated, default: all).
    /// One or more of: jmdict, jmnedict, kanjidic, radkfile,
    /// kradfile, kanjivg, kokugo.
    #[arg(long, value_delimiter = ',')]
    only: Vec<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let wanted = wanted_datasets(&args.only)?;
    // All selected datasets in SOURCES order (default: everything,
    // including kokugo); kanjivg unpacks separately below. `.gz`
    // URLs decode on the fly, anything else downloads raw; present
    // files are skipped.
    let multi = MultiProgress::new();
    for src in SOURCES.iter().filter(|s| wanted.contains(s.key)) {
        let dest = args.out_dir.join(src.file);
        if dest.exists() {
            eprintln!("present, skipping: {}", dest.display());
            continue;
        }
        if src.url.ends_with(".gz") {
            fetch_gz(&multi, src, &args.out_dir)?;
        } else {
            fetch_raw(&dest, src.url)?;
        }
    }
    if wanted.contains("kanjivg") {
        let dir = args.out_dir.join("kanjivg");
        std::fs::create_dir_all(&dir)?;
        fetch_kanjivg(&multi, &dir)?;
    }
    println!("done -> {}", args.out_dir.display());
    Ok(())
}

/// Datasets for --out-dir (--only filters, default all).
fn wanted_datasets(only: &[String]) -> Result<BTreeSet<&str>, Box<dyn std::error::Error>> {
    if only.is_empty() {
        return Ok(KEYS.iter().copied().collect());
    }
    for key in only {
        if !KEYS.contains(&key.as_str()) {
            return Err(format!(
                "unknown dataset {key:?} (expected one of: {})",
                KEYS.join(", ")
            )
            .into());
        }
    }
    Ok(only.iter().map(String::as_str).collect())
}

fn fetch_gz(
    multi: &MultiProgress,
    src: &Source,
    dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    // Non-2xx is an error here (no silent HTML error pages saved as data).
    let mut resp = ureq::get(src.url).call()?;
    let bar = bar_for(multi, src.file, content_length(resp.headers()));
    let out = std::fs::File::create(dir.join(src.file))?;
    // Bytes are counted as the decoder pulls them through, so the bar
    // tracks the download while the file receives plain XML/text.
    let counting = bar.wrap_read(resp.body_mut().as_reader());
    let mut decoder = flate2::read::GzDecoder::new(counting);
    let mut out = std::io::BufWriter::new(out);
    std::io::copy(&mut decoder, &mut out)?;
    // Flush through BufWriter before declaring success; drop order
    // (out, then decoder) is fine since decoding already finished.
    drop(out);
    bar.finish_with_message(format!("{} done", src.file));
    Ok(())
}

/// Latest KanjiVG `*-all.zip`: resolve the `latest` redirect to its
/// tag, find the all-in-one asset, download it with progress, unpack
/// `kanji/*.svg` into `dir`, stamp the release tag as
/// `version.txt`, and remove the zip.
fn fetch_kanjivg(multi: &MultiProgress, dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let tag = latest_kanjivg_tag()?;
    let asset = all_zip_asset(&tag)?;
    let zip_path = dir.join("kanjivg-all.zip");
    download(multi, &asset, &zip_path, "kanjivg-all.zip")?;
    let n = unzip_kanji(multi, &zip_path, dir)?;
    std::fs::remove_file(&zip_path)?;
    std::fs::write(dir.join("version.txt"), format!("{tag}\n"))?;
    println!("kanjivg {tag}: {n} SVGs -> {}", dir.display());
    Ok(())
}

/// Follow `releases/latest` to its tag (e.g. `r20260714`).
fn latest_kanjivg_tag() -> Result<String, Box<dyn std::error::Error>> {
    let resp = ureq::get("https://github.com/KanjiVG/kanjivg/releases/latest").call()?;
    resp.get_uri()
        .path()
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "cannot parse latest KanjiVG release tag".into())
}

/// Scrape the release's asset list for the all-in-one zip
/// (`.../releases/download/<tag>/<name>-all.zip`).
fn all_zip_asset(tag: &str) -> Result<String, Box<dyn std::error::Error>> {
    use std::io::Read as _;
    let url = format!("https://github.com/KanjiVG/kanjivg/releases/expanded_assets/{tag}");
    let mut html = String::new();
    ureq::get(&url)
        .call()?
        .body_mut()
        .as_reader()
        .read_to_string(&mut html)?;
    let end = html
        .find("-all.zip")
        .ok_or("no *-all.zip asset on the latest KanjiVG release")?
        + "-all.zip".len();
    // The href value starts at the download path (no quotes involved).
    let start = html[..end]
        .rfind("/KanjiVG/kanjivg/releases/download/")
        .ok_or("malformed KanjiVG asset link")?;
    Ok(format!("https://github.com{}", &html[start..end]))
}

/// Fetch one kokugo source (skips present files; `.gz` must sniff
/// as gzip, like the reference importer).
fn fetch_raw(dest: &Path, url: &str) -> Result<(), Box<dyn std::error::Error>> {
    if dest.exists() {
        eprintln!("present, skipping: {}", dest.display());
        return Ok(());
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let multi = MultiProgress::new();
    let part = dest.with_extension("part");
    download(&multi, url, &part, &dest.display().to_string())?;
    if dest.extension().is_some_and(|e| e == "gz") {
        use std::io::Read as _;
        let mut magic = [0u8; 2];
        std::fs::File::open(&part)?.read_exact(&mut magic)?;
        if magic != [0x1f, 0x8b] {
            std::fs::remove_file(&part)?;
            return Err(format!("not gzip data: {url}").into());
        }
    }
    std::fs::rename(&part, dest)?;
    Ok(())
}

/// Plain download with a progress bar (no decoding).
fn download(
    multi: &MultiProgress,
    url: &str,
    dest: &Path,
    label: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut resp = ureq::get(url).call()?;
    let bar = bar_for(multi, label, content_length(resp.headers()));
    let out = std::fs::File::create(dest)?;
    let mut out = std::io::BufWriter::new(out);
    std::io::copy(&mut bar.wrap_read(resp.body_mut().as_reader()), &mut out)?;
    drop(out);
    bar.finish_with_message(format!("{label} done"));
    Ok(())
}

/// Unpack `kanji/*.svg` from the zip into `dir` (flattened,
/// zip-slip-safe via `enclosed_name`), with per-file progress.
/// Returns the SVG count.
fn unzip_kanji(
    multi: &MultiProgress,
    zip_path: &Path,
    dir: &Path,
) -> Result<usize, Box<dyn std::error::Error>> {
    let file = std::fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let bar = bar_for(multi, "unpacking", Some(archive.len() as u64));
    let mut n = 0usize;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        // Accept only top-level per-glyph SVGs; `enclosed_name`
        // rejects absolute paths and `..` escapes, and flattening to
        // the file name drops the `kanji/` prefix.
        let dest = match entry.enclosed_name() {
            Some(p)
                if p.starts_with("kanji")
                    && p.extension().and_then(|e| e.to_str()) == Some("svg") =>
            {
                p.file_name().map(|b| dir.join(b))
            }
            _ => None,
        };
        if let Some(dest) = dest {
            let mut out = std::fs::File::create(dest)?;
            std::io::copy(&mut entry, &mut out)?;
            n += 1;
        }
        bar.inc(1);
    }
    bar.finish_with_message(format!("{n} svg"));
    Ok(n)
}

/// Progress bar for a transfer, or a spinner when the server hides
/// the length (a bogus 0-byte bar would be worse than none).
fn bar_for(multi: &MultiProgress, label: &str, total: Option<u64>) -> ProgressBar {
    let bar = multi.add(match total {
        Some(n) => ProgressBar::new(n),
        None => ProgressBar::new_spinner(),
    });
    bar.set_style(
        ProgressStyle::with_template(BAR_TEMPLATE)
            .unwrap()
            .progress_chars("=>-"),
    );
    bar.set_message(label.to_string());
    bar
}

/// `content-length` response header, when the server sends one.
fn content_length(headers: &ureq::http::HeaderMap) -> Option<u64> {
    headers
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
}

//! JLPT word levels (vendored tanos snapshot): exact (keb, reb) pairs, kana-form
//! readings, and keb-only fallbacks. Conflicts resolve to the
//! easiest level; misses are 0 (unknown).

use std::collections::HashMap;

#[derive(Default)]
pub(crate) struct JlptIndex {
    pair: HashMap<(String, String), u8>,
    kana: HashMap<String, u8>,
    keb_only: HashMap<String, u8>,
}

impl JlptIndex {
    pub(crate) fn load(path: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        let mut out = Self::default();
        let mut reader = csv::Reader::from_path(path)?;
        for row in reader.records() {
            let row = row?;
            let level: u8 = row[0].parse()?;
            if !(1..=5).contains(&level) {
                return Err(format!("jlpt.csv: bad level {}", &row[0]).into());
            }
            let keb = row[1].to_string();
            let reb = row[2].to_string();
            if keb.is_empty() {
                out.insert_kana(&reb, level);
            } else if reb.is_empty() {
                out.insert_keb(&keb, level);
            } else {
                out.insert_pair(&keb, &reb, level);
            }
        }
        Ok(out)
    }

    fn insert_pair(&mut self, keb: &str, reb: &str, level: u8) {
        self.pair
            .entry((keb.into(), reb.into()))
            .and_modify(|l| *l = (*l).min(level))
            .or_insert(level);
    }

    fn insert_kana(&mut self, reb: &str, level: u8) {
        self.kana
            .entry(reb.into())
            .and_modify(|l| *l = (*l).min(level))
            .or_insert(level);
    }

    fn insert_keb(&mut self, keb: &str, level: u8) {
        self.keb_only
            .entry(keb.into())
            .and_modify(|l| *l = (*l).min(level))
            .or_insert(level);
    }

    /// Easiest level matching any headword form (0 = unknown).
    pub(crate) fn level(&self, keb: &[String], reb: &[String]) -> u8 {
        let mut best = 0u8;
        let mut consider = |l: Option<&u8>| {
            if let Some(&l) = l {
                best = if best == 0 { l } else { best.min(l) };
            }
        };
        for k in keb {
            consider(self.keb_only.get(k));
            for r in reb {
                consider(self.pair.get(&(k.clone(), r.clone())));
            }
        }
        for r in reb {
            consider(self.kana.get(r));
        }
        best
    }
}

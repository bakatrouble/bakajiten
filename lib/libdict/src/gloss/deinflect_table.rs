//! Rule table: compact `[a]/[i]/[u]/[e]/[o]` patterns expand
//! over the godan row map (§1) at first use, then bucket by final
//! kana of `from` for runtime lookup (§4). Buckets hold
//! longest-`from`-first rules; construction asserts the §4 shape
//! (382 rules, every rule in exactly its ending bucket). Display
//! `reason` names follow zkanji's `wordInflection` vocabulary;
//! connector-only hops keep engine-internal names (see the doc).

use std::collections::HashMap;
use std::sync::LazyLock;

use super::deinflect::{ClassSet, InflClass as Class};

/// One compiled rule: hiragana suffix rewrite plus the class claims
/// it adds (empty = transparent connector, state unchanged) and its
/// display reason (`食べて → 食べる · conjunctive`). `expanded` marks
/// godan-row instances: on equal `from` length, literal rules win
/// (ichidan られる→る "potential / passive" beats row-5r られる→る
/// "passive" for たべられる).
pub struct Rule {
    pub from: String,
    pub to: String,
    pub out: ClassSet,
    pub reason: &'static str,
    pub expanded: bool,
}

/// Pattern output: the row class (`Row`, for `[vowel]` patterns),
/// row expansion with a fixed claim set (`RowSet` — when the
/// dictionary form's class doesn't follow the stem row), a fixed
/// claim set, or transparent (`Open` — state unchanged).
enum Out {
    Row,
    RowSet(&'static [Class]),
    Set(&'static [Class]),
    Open,
}

struct Pattern(&'static str, &'static str, Out, &'static str);

// Shared claim sets (see the doc for membership).
static OUT_V1: &[Class] = &[Class::V1];
static OUT_VK: &[Class] = &[Class::Vk];
static OUT_VS: &[Class] = &[Class::Vs];
static OUT_VZ: &[Class] = &[Class::Vz];
static OUT_5ARU: &[Class] = &[Class::V5aru];
static OUT_ADJI: &[Class] = &[Class::AdjI];
static OUT_V1K: &[Class] = &[Class::V1, Class::Vk];
static OUT_V1_5ARU: &[Class] = &[Class::V1, Class::V5aru];
static OUT_5K_5KS: &[Class] = &[Class::V5k, Class::V5kS];
/// All nine godan rows (まい→∅ volitional negative).
static OUT_GODAN: &[Class] = &[
    Class::V5k,
    Class::V5g,
    Class::V5s,
    Class::V5t,
    Class::V5n,
    Class::V5b,
    Class::V5m,
    Class::V5r,
    Class::V5u,
];
/// Prohibitive な→∅: every verb class but the specials.
static OUT_NA: &[Class] = &[
    Class::V1,
    Class::V5k,
    Class::V5g,
    Class::V5s,
    Class::V5t,
    Class::V5n,
    Class::V5b,
    Class::V5m,
    Class::V5r,
    Class::V5u,
    Class::Vk,
    Class::Vs,
];
/// Explanatory んです/んだ→∅: every verb class plus i-adjectives.
static OUT_VERBS_ADJI: &[Class] = &[
    Class::V1,
    Class::V5k,
    Class::V5g,
    Class::V5s,
    Class::V5t,
    Class::V5n,
    Class::V5b,
    Class::V5m,
    Class::V5r,
    Class::V5u,
    Class::V5kS,
    Class::V5aru,
    Class::Vk,
    Class::Vs,
    Class::AdjI,
    Class::Vz,
];

/// Godan row map (§1): class plus a/i/u/e/o kana.
const ROWS: [(Class, [char; 5]); 9] = [
    (Class::V5k, ['か', 'き', 'く', 'け', 'こ']),
    (Class::V5g, ['が', 'ぎ', 'ぐ', 'げ', 'ご']),
    (Class::V5s, ['さ', 'し', 'す', 'せ', 'そ']),
    (Class::V5t, ['た', 'ち', 'つ', 'て', 'と']),
    (Class::V5n, ['な', 'に', 'ぬ', 'ね', 'の']),
    (Class::V5b, ['ば', 'び', 'ぶ', 'べ', 'ぼ']),
    (Class::V5m, ['ま', 'み', 'む', 'め', 'も']),
    (Class::V5r, ['ら', 'り', 'る', 'れ', 'ろ']),
    (Class::V5u, ['わ', 'い', 'う', 'え', 'お']),
];

/// The table: (from → to, out, reason). `[vowel]` expands ×9.
const PATTERNS: &[Pattern] = &[
    // A. Polite (masu-stem + suffix).
    Pattern("[i]ます", "[u]", Out::Row, "polite"),
    Pattern("[i]ません", "[u]", Out::Row, "polite neg."),
    Pattern("[i]ました", "[u]", Out::Row, "polite past"),
    Pattern(
        "[i]ましょう",
        "[u]",
        Out::Row,
        "polite volitional / tentative",
    ),
    Pattern("ます", "る", Out::Set(OUT_V1), "polite"),
    Pattern("ません", "る", Out::Set(OUT_V1), "polite neg."),
    Pattern("ました", "る", Out::Set(OUT_V1), "polite past"),
    Pattern(
        "ましょう",
        "る",
        Out::Set(OUT_V1),
        "polite volitional / tentative",
    ),
    Pattern("きます", "くる", Out::Set(OUT_VK), "polite"),
    Pattern("きません", "くる", Out::Set(OUT_VK), "polite"),
    Pattern("きました", "くる", Out::Set(OUT_VK), "polite"),
    Pattern("きましょう", "くる", Out::Set(OUT_VK), "polite"),
    Pattern("します", "する", Out::Set(OUT_VS), "polite"),
    Pattern("しません", "する", Out::Set(OUT_VS), "polite"),
    Pattern("しました", "する", Out::Set(OUT_VS), "polite"),
    Pattern("しましょう", "する", Out::Set(OUT_VS), "polite"),
    Pattern("ざいます", "ざる", Out::Set(OUT_5ARU), "polite"),
    Pattern("さいます", "さる", Out::Set(OUT_5ARU), "polite"),
    // B. Negative.
    Pattern("[a]ない", "[u]", Out::Row, "neg."),
    Pattern("ない", "る", Out::Set(OUT_V1K), "neg."),
    Pattern("こない", "くる", Out::Set(OUT_VK), "neg."),
    Pattern("しない", "する", Out::Set(OUT_VS), "neg."),
    Pattern("くない", "い", Out::Set(OUT_ADJI), "neg."),
    Pattern("じない", "ずる", Out::Set(OUT_VZ), "neg."),
    Pattern("さない", "さる", Out::Set(OUT_5ARU), "neg."),
    Pattern("せん", "する", Out::Set(OUT_VS), "neg."),
    Pattern("せん", "ない", Out::Set(OUT_V1), "neg."),
    Pattern("ず", "る", Out::Set(OUT_V1K), "without / neg. conjunctive"),
    Pattern("ずに", "る", Out::Set(OUT_V1), "without / neg. conjunctive"),
    Pattern("ぬ", "る", Out::Set(OUT_V1), "archaic neg."),
    Pattern("[a]ぬ", "[u]", Out::Row, "archaic neg."),
    Pattern("ないで", "ない", Out::Open, "without / neg. request"),
    Pattern("まい", "", Out::Set(OUT_GODAN), "volitional negative"),
    Pattern("まい", "る", Out::Set(OUT_V1K), "volitional negative"),
    // C. Past / te-form.
    Pattern("いて", "く", Out::Set(&[Class::V5k]), "conjunctive"),
    Pattern("いで", "ぐ", Out::Set(&[Class::V5g]), "conjunctive"),
    Pattern("して", "す", Out::Set(&[Class::V5s]), "conjunctive"),
    Pattern("って", "う", Out::Set(&[Class::V5u]), "conjunctive"),
    Pattern("って", "つ", Out::Set(&[Class::V5t]), "conjunctive"),
    Pattern("って", "る", Out::Set(&[Class::V5r]), "conjunctive"),
    Pattern("んで", "ぬ", Out::Set(&[Class::V5n]), "conjunctive"),
    Pattern("んで", "ぶ", Out::Set(&[Class::V5b]), "conjunctive"),
    Pattern("んで", "む", Out::Set(&[Class::V5m]), "conjunctive"),
    Pattern("いた", "く", Out::Set(&[Class::V5k]), "past"),
    Pattern("いだ", "ぐ", Out::Set(&[Class::V5g]), "past"),
    Pattern("した", "す", Out::Set(&[Class::V5s]), "past"),
    Pattern("った", "う", Out::Set(&[Class::V5u]), "past"),
    Pattern("った", "つ", Out::Set(&[Class::V5t]), "past"),
    Pattern("った", "る", Out::Set(&[Class::V5r]), "past"),
    Pattern("んだ", "ぬ", Out::Set(&[Class::V5n]), "past"),
    Pattern("んだ", "ぶ", Out::Set(&[Class::V5b]), "past"),
    Pattern("んだ", "む", Out::Set(&[Class::V5m]), "past"),
    Pattern("いって", "いく", Out::Set(OUT_5K_5KS), "conjunctive"),
    Pattern("いった", "いく", Out::Set(OUT_5K_5KS), "past"),
    Pattern("た", "る", Out::Set(OUT_V1), "past"),
    Pattern("て", "る", Out::Set(OUT_V1), "conjunctive"),
    Pattern("きた", "くる", Out::Set(OUT_VK), "past"),
    Pattern("きて", "くる", Out::Set(OUT_VK), "conjunctive"),
    // No bare た→くる / て→くる: くる's own forms are きた/きて
    // above; the bare endings only ever peel る-verbs (たべた→
    // たべる) and otherwise hallucinate compounds (てす→くる).
    Pattern("した", "する", Out::Set(OUT_VS), "past"),
    Pattern("して", "する", Out::Set(OUT_VS), "conjunctive"),
    Pattern("じた", "ずる", Out::Set(OUT_VZ), "past"),
    Pattern("じて", "ずる", Out::Set(OUT_VZ), "conjunctive"),
    Pattern("かった", "い", Out::Set(OUT_ADJI), "past"),
    Pattern("くて", "い", Out::Set(OUT_ADJI), "conjunctive"),
    Pattern("さった", "さる", Out::Set(OUT_5ARU), "past"),
    Pattern("さって", "さる", Out::Set(OUT_5ARU), "conjunctive"),
    // D. Conditionals and concessives.
    Pattern("たら", "た", Out::Open, "-tara conditional"),
    Pattern("だら", "だ", Out::Open, "-tara conditional"),
    Pattern("たり", "た", Out::Open, "partial conjunctive"),
    Pattern("だり", "だ", Out::Open, "partial conjunctive"),
    Pattern("[e]ば", "[u]", Out::Row, "-ba conditional"),
    Pattern("れば", "る", Out::Set(OUT_V1_5ARU), "-ba conditional"),
    Pattern("くれば", "くる", Out::Set(OUT_VK), "-ba conditional"),
    Pattern("すれば", "する", Out::Set(OUT_VS), "-ba conditional"),
    Pattern("ければ", "い", Out::Set(OUT_ADJI), "-ba conditional"),
    Pattern("ても", "て", Out::Open, "concessive"),
    Pattern("でも", "で", Out::Open, "concessive"),
    Pattern("ては", "て", Out::Open, "conditional-は"),
    Pattern("では", "で", Out::Open, "conditional-は"),
    Pattern("くても", "い", Out::Set(OUT_ADJI), "concessive"),
    Pattern("なくちゃ", "ない", Out::Open, "must / neg. -ba cond."),
    Pattern("なくては", "ない", Out::Open, "must / neg. -ba cond."),
    Pattern("りゃ", "れば", Out::Open, "colloquial contraction"),
    Pattern(
        "きゃ",
        "けば",
        Out::Set(&[Class::V5k]),
        "colloquial ba-contraction",
    ),
    Pattern(
        "ぎゃ",
        "げば",
        Out::Set(&[Class::V5g]),
        "colloquial ba-contraction",
    ),
    Pattern(
        "しゃ",
        "せば",
        Out::Set(&[Class::V5s]),
        "colloquial ba-contraction",
    ),
    Pattern(
        "ちゃ",
        "てば",
        Out::Set(&[Class::V5t]),
        "colloquial ba-contraction",
    ),
    Pattern(
        "にゃ",
        "ねば",
        Out::Set(&[Class::V5n]),
        "colloquial ba-contraction",
    ),
    Pattern(
        "びゃ",
        "べば",
        Out::Set(&[Class::V5b]),
        "colloquial ba-contraction",
    ),
    Pattern(
        "みゃ",
        "めば",
        Out::Set(&[Class::V5m]),
        "colloquial ba-contraction",
    ),
    // E. Voice: passive / causative / potential.
    Pattern("[a]れる", "[u]", Out::Row, "passive"),
    Pattern("[a]せる", "[u]", Out::Row, "causative"),
    Pattern("[e]る", "[u]", Out::Row, "potential"),
    Pattern("られる", "る", Out::Set(OUT_V1), "potential / passive"),
    Pattern("させる", "る", Out::Set(OUT_V1), "causative"),
    Pattern("こられる", "くる", Out::Set(OUT_VK), "potential / passive"),
    Pattern("こさせる", "くる", Out::Set(OUT_VK), "causative"),
    Pattern("される", "する", Out::Set(OUT_VS), "passive causative"),
    Pattern("れる", "る", Out::Set(OUT_V1), "potential"),
    Pattern("[a]す", "[u]", Out::Row, "causative"),
    // Short-causative peel for a-stem godan-る verbs
    // (かかす→かかる): §5's かかされる chain needs it, and the
    // plain す→る inversion over-fires (てす→てる). The stem row
    // can't name the class (かかる is V5r, not V5k), so the
    // dictionary form's own row does: V5r, its only class.
    Pattern("[a]す", "[a]る", Out::RowSet(&[Class::V5r]), "causative"),
    Pattern("じられる", "ずる", Out::Set(OUT_VZ), "potential / passive"),
    Pattern("じさせる", "ずる", Out::Set(OUT_VZ), "causative"),
    // F. Volitional / imperative / directives.
    Pattern("[o]う", "[u]", Out::Row, "volitional / tentative"),
    Pattern("よう", "る", Out::Set(OUT_V1), "volitional / tentative"),
    Pattern("こよう", "くる", Out::Set(OUT_VK), "volitional / tentative"),
    Pattern("しよう", "する", Out::Set(OUT_VS), "volitional / tentative"),
    Pattern("かろう", "い", Out::Set(OUT_ADJI), "volitional / tentative"),
    Pattern("け", "く", Out::Set(&[Class::V5k]), "command"),
    Pattern("げ", "ぐ", Out::Set(&[Class::V5g]), "command"),
    Pattern("せ", "す", Out::Set(&[Class::V5s]), "command"),
    Pattern("て", "つ", Out::Set(&[Class::V5t]), "command"),
    Pattern("ね", "ぬ", Out::Set(&[Class::V5n]), "command"),
    Pattern("べ", "ぶ", Out::Set(&[Class::V5b]), "command"),
    Pattern("め", "む", Out::Set(&[Class::V5m]), "command"),
    Pattern("れ", "る", Out::Set(&[Class::V5r]), "command"),
    Pattern("え", "う", Out::Set(&[Class::V5u]), "command"),
    Pattern("ろ", "る", Out::Set(OUT_V1), "command"),
    Pattern("よ", "る", Out::Set(OUT_V1), "command"),
    Pattern("こい", "くる", Out::Set(OUT_VK), "command"),
    Pattern("しろ", "する", Out::Set(OUT_VS), "command"),
    Pattern("せよ", "する", Out::Set(OUT_VS), "command"),
    Pattern("じろ", "ずる", Out::Set(OUT_VZ), "command"),
    Pattern("じよう", "ずる", Out::Set(OUT_VZ), "volitional / tentative"),
    Pattern("じれば", "ずる", Out::Set(OUT_VZ), "-ba conditional"),
    Pattern("され", "さる", Out::Set(OUT_5ARU), "command"),
    Pattern("[i]なさい", "[u]", Out::Row, "command"),
    Pattern("なさい", "る", Out::Set(OUT_V1K), "command"),
    Pattern("ださい", "ださる", Out::Set(OUT_5ARU), "command"),
    Pattern("な", "", Out::Set(OUT_NA), "prohibitive"),
    // G. Desire and evaluative auxiliaries (all on the masu-stem).
    Pattern("たい", "る", Out::Set(OUT_V1), "intent"),
    Pattern("[i]たい", "[u]", Out::Row, "intent"),
    Pattern("きたい", "くる", Out::Set(OUT_VK), "intent"),
    Pattern("したい", "する", Out::Set(OUT_VS), "intent"),
    Pattern("すぎる", "る", Out::Set(OUT_V1), "excessive"),
    Pattern("[i]すぎる", "[u]", Out::Row, "excessive"),
    Pattern("きすぎる", "くる", Out::Set(OUT_VK), "excessive kuru"),
    Pattern("しすぎる", "する", Out::Set(OUT_VS), "excessive suru"),
    Pattern("すぎ", "る", Out::Set(OUT_V1), "excessive"),
    Pattern("[i]すぎ", "[u]", Out::Row, "excessive"),
    Pattern("やすい", "る", Out::Set(OUT_V1), "ease"),
    Pattern("[i]やすい", "[u]", Out::Row, "ease"),
    Pattern("しやすい", "する", Out::Set(OUT_VS), "ease suru"),
    Pattern("きやすい", "くる", Out::Set(OUT_VK), "ease kuru"),
    Pattern("にくい", "る", Out::Set(OUT_V1), "difficulty"),
    Pattern("[i]にくい", "[u]", Out::Row, "difficulty"),
    Pattern("しにくい", "する", Out::Set(OUT_VS), "difficulty suru"),
    Pattern("きにくい", "くる", Out::Set(OUT_VK), "difficulty kuru"),
    Pattern("ながら", "る", Out::Set(OUT_V1), "while"),
    Pattern("[i]ながら", "[u]", Out::Row, "while"),
    Pattern("しながら", "する", Out::Set(OUT_VS), "while suru"),
    Pattern("きながら", "くる", Out::Set(OUT_VK), "while kuru"),
    Pattern("つつある", "る", Out::Set(OUT_V1), "formal progressive"),
    Pattern("[i]つつある", "[u]", Out::Row, "formal progressive"),
    Pattern(
        "しつつある",
        "する",
        Out::Set(OUT_VS),
        "formal progressive suru",
    ),
    Pattern("がる", "い", Out::Set(OUT_ADJI), "3rd intent"),
    Pattern("そうだ", "る", Out::Set(OUT_V1), "appearance"),
    Pattern("そうだ", "い", Out::Set(OUT_ADJI), "appearance"),
    Pattern("[i]そうだ", "[u]", Out::Row, "appearance"),
    Pattern("しそうだ", "する", Out::Set(OUT_VS), "appearance"),
    // H. Contractions and te-compounds (all transparent connectors).
    Pattern("てる", "て", Out::Open, "continuous"),
    Pattern("でる", "で", Out::Open, "continuous"),
    Pattern("ちゃう", "て", Out::Open, "finished / bothersome"),
    Pattern("じゃう", "で", Out::Open, "finished / bothersome"),
    Pattern("ちゃった", "た", Out::Open, "finished / bothersome"),
    Pattern("じゃった", "だ", Out::Open, "finished / bothersome"),
    Pattern("とく", "て", Out::Open, "do in advance / keep ~ing"),
    Pattern("どく", "で", Out::Open, "do in advance / keep ~ing"),
    Pattern("てみる", "て", Out::Open, "attempt"),
    Pattern("でみる", "で", Out::Open, "attempt"),
    Pattern("てくる", "て", Out::Open, "directional aspect"),
    Pattern("でくる", "で", Out::Open, "directional aspect"),
    Pattern("ていく", "て", Out::Open, "directional aspect"),
    Pattern("でいく", "で", Out::Open, "directional aspect"),
    Pattern("てゆく", "て", Out::Open, "directional aspect"),
    Pattern("でゆく", "で", Out::Open, "directional aspect"),
    Pattern("てしまう", "て", Out::Open, "finished / bothersome"),
    Pattern("でしまう", "で", Out::Open, "finished / bothersome"),
    Pattern("てほしい", "て", Out::Open, "desire"),
    Pattern("でほしい", "で", Out::Open, "desire"),
    Pattern("てあげる", "て", Out::Open, "benefactive"),
    Pattern("てくれる", "て", Out::Open, "benefactive"),
    Pattern("てもらう", "て", Out::Open, "benefactive"),
    Pattern("であげる", "で", Out::Open, "benefactive"),
    Pattern("でくれる", "で", Out::Open, "benefactive"),
    Pattern("でもらう", "で", Out::Open, "benefactive"),
    Pattern("ていただく", "て", Out::Open, "humble"),
    Pattern("ていただきます", "て", Out::Open, "humble"),
    Pattern("せて", "せ", Out::Open, "conjunctive"),
    Pattern("られて", "られ", Out::Open, "conjunctive"),
    Pattern("れて", "れ", Out::Open, "conjunctive"),
    Pattern("きています", "くる", Out::Open, "polite progressive kuru"),
    Pattern("きていました", "くる", Out::Open, "polite progressive kuru"),
    Pattern("きていません", "くる", Out::Open, "polite progressive kuru"),
    Pattern("しています", "する", Out::Open, "polite progressive suru"),
    Pattern("していました", "する", Out::Open, "polite progressive suru"),
    Pattern("していません", "する", Out::Open, "polite progressive suru"),
    Pattern("てある", "て", Out::Open, "stative"),
    Pattern("である", "で", Out::Open, "stative"),
    Pattern("ください", "", Out::Open, "ください strip"),
    Pattern("なきゃ", "なければ", Out::Open, "must / neg. -ba cond."),
    // Obligation ならない-strip: in §5's worked chains but missing
    // from the §3 tables (いかなければならない → いかなければ).
    // Transparent: later hops add the classes.
    Pattern("ならない", "", Out::Open, "obligation"),
    // I. Copula and polite tails.
    Pattern("です", "", Out::Open, "copula"),
    Pattern("でした", "", Out::Open, "copula"),
    Pattern("んです", "", Out::Set(OUT_VERBS_ADJI), "explanatory"),
    Pattern("んだ", "", Out::Set(OUT_VERBS_ADJI), "explanatory"),
];

/// `from` ending → rules, longest-`from`-first (table order breaks
/// ties, so the doc's first-listed reason wins first discovery).
/// Construction asserts the §4 shape: 382 rules (372 tabled plus
/// the ならない obligation strip §5 needs and the short-causative
/// す peel — now expanded over its godan row — its かかされる
/// chain needs), non-empty `from`
/// suffixes, every rule in exactly its ending bucket.
static BUCKETS: LazyLock<HashMap<char, Vec<Rule>>> = LazyLock::new(|| {
    let mut buckets: HashMap<char, Vec<Rule>> = HashMap::new();
    let mut total = 0;
    let mut push =
        |from: String, to: String, out: ClassSet, reason: &'static str, expanded: bool| {
            assert!(
                !from.is_empty(),
                "deinflect: empty `from` would match everything"
            );
            let last = from.chars().next_back().expect("checked");
            buckets.entry(last).or_default().push(Rule {
                from,
                to,
                out,
                reason,
                expanded,
            });
            total += 1;
        };
    for Pattern(from, to, out, reason) in PATTERNS {
        match out {
            Out::Open => push(
                from.to_string(),
                to.to_string(),
                ClassSet::empty(),
                reason,
                false,
            ),
            Out::Set(classes) => push(
                from.to_string(),
                to.to_string(),
                ClassSet::of(classes),
                reason,
                false,
            ),
            Out::Row => {
                for (class, row) in ROWS {
                    let (f, t) = expand_row(from, to, row);
                    push(f, t, ClassSet::of(&[class]), reason, true);
                }
            }
            Out::RowSet(classes) => {
                for (_, row) in ROWS {
                    let (f, t) = expand_row(from, to, row);
                    push(f, t, ClassSet::of(classes), reason, true);
                }
            }
        }
    }
    assert_eq!(total, 382, "deinflect: rule count drifted from the doc");
    for rules in buckets.values_mut() {
        rules.sort_by(|a, b| {
            b.from
                .len()
                .cmp(&a.from.len())
                .then_with(|| a.expanded.cmp(&b.expanded))
        });
    }
    buckets
});

/// Expand one `[a]/[i]/[u]/[e]/[o]` pattern over a godan row.
fn expand_row(from: &str, to: &str, row: [char; 5]) -> (String, String) {
    let mut f = from.to_string();
    let mut t = to.to_string();
    for (vowel, kana) in ["a", "i", "u", "e", "o"].iter().zip(row.iter()) {
        let hole = format!("[{vowel}]");
        f = f.replace(hole.as_str(), &kana.to_string());
        t = t.replace(hole.as_str(), &kana.to_string());
    }
    (f, t)
}

/// Rules whose `from` ends in `last` (the surface's final kana),
/// longest-`from`-first. Empty when no rule ends that way (fast
/// path: most queries skip the BFS after one lookup).
pub fn rules_for(last: char) -> &'static [Rule] {
    static EMPTY: &[Rule] = &[];
    BUCKETS.get(&last).map(Vec::as_slice).unwrap_or(EMPTY)
}

/// Total rule count (bucket-shape test below).
pub fn rule_count() -> usize {
    BUCKETS.values().map(Vec::len).sum()
}

#[cfg(test)]
mod table_tests {
    use super::*;

    #[test]
    fn table_matches_doc_shape() {
        // §4: 382 rules, every rule in exactly the bucket keyed by
        // its `from` ending.
        assert_eq!(rule_count(), 382);
        for (last, rules) in BUCKETS.iter() {
            for rule in rules {
                assert_eq!(rule.from.chars().next_back(), Some(*last));
            }
            // Longest-`from`-first inside each bucket.
            let mut lens = rules.iter().map(|r| r.from.len());
            let mut prev = usize::MAX;
            for len in lens.by_ref() {
                assert!(len <= prev);
                prev = len;
            }
        }
    }
}

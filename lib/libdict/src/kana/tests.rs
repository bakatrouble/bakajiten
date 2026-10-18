//! Acceptance traces for batch conversion and incremental display,
//! row-for-row.

use super::convert::{display, to_kana};
use super::table::TABLE;

#[test]
fn table_sorted_for_binary_search() {
    assert!(TABLE.windows(2).all(|w| w[0].0 < w[1].0));
}

#[test]
fn batch_acceptance() {
    #[rustfmt::skip]
    let rows = [
        ("a", "あ"), ("shi", "し"), ("chi", "ち"), ("tsu", "つ"),
        ("fu", "ふ"), ("wo", "を"), ("n", "ん"), ("si", "し"),
        ("ti", "ち"), ("tu", "つ"), ("hu", "ふ"), ("zi", "じ"),
        ("sya", "しゃ"), ("sha", "しゃ"), ("sho", "しょ"), ("shu", "しゅ"),
        ("tyu", "ちゅ"), ("zyo", "じょ"),
        ("di", "ぢ"), ("du", "づ"), ("dya", "ぢゃ"),
        ("kya", "きゃ"), ("shu", "しゅ"), ("cho", "ちょ"),
        ("nyu", "にゅ"), ("ja", "じゃ"), ("byo", "びょ"),
        ("konnichiwa", "こんにちは"), ("konbanwa", "こんばんは"),
        ("arigatou", "ありがとう"), ("gakkou", "がっこう"),
        ("shinjuku", "しんじゅく"), ("matcha", "まっちゃ"),
        ("kissaten", "きっさてん"), ("beddo", "べっど"),
        ("emma", "えんま"), ("komatta", "こまった"),
        ("kinen", "きねん"), ("ginen", "ぎねん"),
        ("kin'en", "きんえん"), ("shin", "しん"), ("hon", "ほん"),
        ("shinya", "しにゃ"), ("shin'ya", "しんや"),
        ("xtsu", "っ"), ("ltu", "っ"), ("xa", "ぁ"),
        ("xyo", "ょ"), ("xwa", "ゎ"), ("fa", "ふぁ"),
        ("vu", "ゔ"), ("thi", "てぃ"), ("dhu", "でゅ"),
        ("wi", "うぃ"), ("ye", "いぇ"),
        ("pa-thi", "ぱーてぃ"), ("食べる", "食べる"),
        ("あ", "あ"), ("漢字", "漢字"),
        ("taberu食べる", "たべる食べる"), ("hello", "へlぉ"),
        ("'", "'"),
    ];
    for (input, expected) in rows {
        assert_eq!(to_kana(input), expected, "batch {input:?}");
    }
}

#[test]
fn display_acceptance() {
    #[rustfmt::skip]
    let rows = [
        ("s", "s"), ("sh", "sh"), ("shi", "し"),
        ("n", "n"), ("na", "な"), ("nn", "んn"), ("nna", "んな"),
        ("t", "t"), ("tc", "tc"), ("tch", "tch"), ("tchi", "っち"),
        ("shin", "しn"), ("shina", "しな"), ("shinya", "しにゃ"),
        ("sha", "しゃ"), ("sho", "しょ"), ("shu", "しゅ"),
        ("shin'a", "しんあ"), ("ka", "か"), ("kat", "かt"),
        ("katta", "かった"), ("katt", "かっt"), ("kat", "かt"),
        ("gakk", "がっk"), ("ko", "こ"), ("kon", "こn"),
        ("konn", "こんn"), ("konni", "こんに"),
        ("konnichi", "こんにち"), ("konnichiw", "こんにちw"),
        ("konnichiwa", "こんにちは"), ("tsu", "つ"), ("du", "づ"),
        ("thi", "てぃ"), ("wo", "を"), ("-", "ー"),
        ("xtsu", "っ"), ("n'", "ん"), ("q", "q"),
    ];
    for (raw, expected) in rows {
        assert_eq!(display(raw), expected, "display {raw:?}");
    }
}

#[test]
fn pending_tail_traces() {
    use super::convert::has_pending_tail;
    // Live tails (§2): single consonants and n wait for more.
    assert!(has_pending_tail("n"));
    assert!(has_pending_tail("shin"));
    assert!(has_pending_tail("sh"));
    assert!(has_pending_tail("t"));
    assert!(has_pending_tail("たn"));
    // Settled buffers convert whole: no tail shows raw.
    assert!(!has_pending_tail(""));
    assert!(!has_pending_tail("ta"));
    assert!(!has_pending_tail("shi"));
    assert!(!has_pending_tail("たべる"));
    assert!(!has_pending_tail("食べる"));
}

//! Acceptance rows for numbered → marked pinyin (placement rules
//! plus wild-data shapes from kanjidic2).

use super::to_marked;

#[test]
fn placement_and_wild_shapes() {
    #[rustfmt::skip]
    let rows = [
        ("ding1", "dīng"), ("ri4", "rì"), ("zhong1", "zhōng"),
        ("shi4", "shì"), ("ai4", "ài"), ("ou1", "ōu"),
        ("ing1", "īng"), ("er2", "ér"), ("ong1", "ōng"),
        ("a1", "ā"), ("wu3", "wǔ"), ("yu4", "yù"),
        ("quan2", "quán"), ("jue2", "jué"), ("yuan2", "yuán"),
        ("gui4", "guì"), ("liu2", "liú"), ("xue1", "xuē"),
        ("nu:3", "nǚ"), ("lu:e4", "lüè"), ("lue4", "luè"), ("DING1", "dīng"),
        ("de5", "de"), ("ma", "ma"), ("m2", "m"),
        ("shitsu", "shitsu"), ("", ""),
    ];
    for (input, expected) in rows {
        assert_eq!(to_marked(input), expected, "pinyin {input:?}");
    }
}

pub mod deinflect;
pub mod deinflect_table;
pub mod display;
pub mod field;
pub mod file;
pub mod gloss_type;
pub mod inf;
pub mod misc;
pub mod name_type;
pub mod pos;
pub mod priority;
pub mod rank;
pub mod record;
pub mod search;
#[cfg(test)]
pub(crate) mod testhelp;
#[cfg(test)]
mod tests;
pub mod view;

pub use deinflect::{ClassSet, Deinflection, InflClass, deinflect, entry_classes};
pub use rank::sort_hits;
pub use search::{ArchivedDictionaryFile, SearchType};

pub use file::{
    ArchivedDictIndex, DictIndex, GLOSS_JMDICT_MAGIC, GLOSS_JMNEDICT_MAGIC, GLOSS_KOKUGO_MAGIC,
    gloss_to_bytes, save_gloss,
};
pub use record::{
    EntryRecord, GlossRecord, IndexRecord, KanjiElementRecord, LangSourceRecord,
    ReadingElementRecord, SenseDetail, SenseExtras, SenseRecord, record_freq,
};
pub use view::{
    GlossView, decode_entry, gloss_from_bytes, gloss_header, gloss_view, index_section,
    sections_unchecked,
};

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, Write},
    path::Path,
};
use unicode_joining_type::{JoiningType::*, get_joining_type};

pub const SAMPLE: &str = "بِسْمِ اللهِ الرَّحْمَٰنِ الرَّحِيمِ";
pub const MAX_CHARS: usize = 256;
pub const MAX_WORD_OFFSET: f64 = 100_000.;
pub const MAX_STRETCH: u8 = 8;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Offset {
    pub x: f64,
    pub y: f64,
}

// Byte offsets always refer to the original UTF-8 text, never to added strokes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Document {
    version: u8,
    pub text: String,
    pub stretches: BTreeMap<usize, u8>,
    #[serde(default)]
    pub style: crate::book::Style,
    #[serde(default)]
    pub words: BTreeMap<usize, Offset>,
    #[serde(default)]
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub letter_inks: BTreeMap<usize, [u8; 3]>,
    #[serde(default)]
    pub letters: BTreeMap<usize, Offset>,
    #[serde(default)]
    pub forms: BTreeMap<usize, String>,
    #[serde(default)]
    pub breaks: BTreeSet<usize>,
    #[serde(default)]
    pub parts: BTreeMap<String, Offset>,
    pub marks: BTreeMap<String, Offset>,
}

impl Default for Document {
    fn default() -> Self {
        Self {
            version: 1,
            text: SAMPLE.into(),
            stretches: BTreeMap::new(),
            style: Default::default(),
            words: Default::default(),
            letter_inks: Default::default(),
            letters: Default::default(),
            forms: Default::default(),
            breaks: Default::default(),
            parts: Default::default(),
            marks: BTreeMap::new(),
        }
    }
}

pub fn joins(text: &str) -> Vec<(usize, String)> {
    let letters: Vec<_> = text
        .char_indices()
        .filter(|(_, c)| get_joining_type(*c) != Transparent)
        .collect();
    letters
        .windows(2)
        .filter_map(|pair| {
            let ((_, left), (index, right)) = (pair[0], pair[1]);
            let connected = matches!(get_joining_type(left), DualJoining | LeftJoining)
                && matches!(get_joining_type(right), DualJoining | RightJoining);
            let lam_alef = left == 'ل' && "اآأإٱ".contains(right);
            (connected && !lam_alef).then(|| (index, format!("{left}ـ{right}")))
        })
        .collect()
}

pub fn valid_text(text: &str) -> bool {
    text.chars().count() <= MAX_CHARS
        && !text
            .chars()
            .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'))
}
impl Document {
    #[cfg(test)]
    pub fn save(&self, path: &Path) -> io::Result<()> {
        atomic_write(path, &serde_json::to_vec_pretty(self)?)
    }
    #[cfg(test)]
    pub fn load(path: &Path) -> Result<Self, String> {
        if std::fs::metadata(path).map_err(|e| e.to_string())?.len() > 512_000 {
            return Err("ملف المشروع أكبر من الحد المسموح".into());
        }
        let doc: Self = serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|_| "تعذرت قراءة ملف مداد")?;
        if !doc.valid() {
            return Err("إصدار الملف أو بياناته غير مدعومة".into());
        }
        Ok(doc)
    }
    pub fn set_word_ink(&mut self, word: usize, color: Option<[u8; 3]>) {
        if let Some((start, text)) = words(&self.text).get(word) {
            for (i, _) in text.char_indices() {
                let key = start + i;
                if let Some(color) = color {
                    self.letter_inks.insert(key, color);
                } else {
                    self.letter_inks.remove(&key);
                }
            }
        }
    }
    pub fn valid(&self) -> bool {
        let doc = self;
        let allowed = joins(&doc.text);
        let valid_mark_key = |key: &str| {
            let mut parts = key.split('/');
            let index = parts.next().and_then(|v| v.parse::<usize>().ok());
            index.is_some_and(|i| i < doc.text.len() && doc.text.is_char_boundary(i))
                && parts.next().is_some_and(|v| v.parse::<u16>().is_ok())
                && parts.next().is_some_and(|v| v.parse::<u16>().is_ok())
                && parts.next().is_none()
        };
        let word_starts: BTreeSet<_> = words(&doc.text).into_iter().map(|(i, _)| i).collect();
        !(doc.version != 1
            || !doc.marks.keys().all(|k| valid_mark_key(k))
            || !valid_text(&doc.text)
            || doc
                .stretches
                .iter()
                .any(|(k, v)| *v > MAX_STRETCH || !allowed.iter().any(|(i, _)| i == k))
            || doc.marks.values().any(|o| {
                !o.x.is_finite() || !o.y.is_finite() || o.x.abs() > 60. || o.y.abs() > 60.
            })
            || !doc.style.valid()
            || doc
                .breaks
                .iter()
                .any(|k| !allowed.iter().any(|(i, _)| k == i))
            || doc
                .words
                .iter()
                .any(|(k, o)| !word_starts.contains(k) || !o.within(MAX_WORD_OFFSET))
            || doc.forms.iter().any(|(k, v)| {
                !word_starts.contains(k) || !doc.style.font.forms().iter().any(|(tag, _)| tag == v)
            })
            || doc
                .letter_inks
                .keys()
                .any(|&i| i >= doc.text.len() || !doc.text.is_char_boundary(i))
            || doc.letters.iter().any(|(i, o)| {
                *i >= doc.text.len() || !doc.text.is_char_boundary(*i) || !o.within(500.)
            })
            || doc.parts.iter().any(|(k, o)| {
                !k.rsplit_once('/')
                    .is_some_and(|(g, n)| valid_mark_key(g) && n.parse::<u16>().is_ok())
                    || !o.within(100.)
            }))
    }
    // Retain placements outside the edited range, even if their byte indices shifted.
    pub fn edit_text(&mut self, new: String) {
        if self.text == new {
            return;
        }
        let prefix: usize = self
            .text
            .chars()
            .zip(new.chars())
            .take_while(|(a, b)| a == b)
            .map(|(a, _)| a.len_utf8())
            .sum();
        let suffix: usize = self.text[prefix..]
            .chars()
            .rev()
            .zip(new[prefix..].chars().rev())
            .take_while(|(a, b)| a == b)
            .map(|(a, _)| a.len_utf8())
            .sum();
        let remap = |idx: usize| {
            if idx < prefix {
                Some(idx)
            } else if idx >= self.text.len() - suffix {
                Some(new.len() - suffix + (idx - (self.text.len() - suffix)))
            } else {
                None
            }
        };
        let allowed = joins(&new);
        self.stretches = self
            .stretches
            .iter()
            .filter_map(|(&i, &v)| {
                let j = remap(i)?;
                allowed.iter().any(|(k, _)| *k == j).then_some((j, v))
            })
            .collect();
        self.marks = self
            .marks
            .iter()
            .filter_map(|(key, off)| {
                let (i, id) = key.split_once('/')?;
                Some((format!("{}/{id}", remap(i.parse().ok()?)?), off.clone()))
            })
            .collect();
        let moved_parts = self
            .parts
            .iter()
            .filter_map(|(key, off)| {
                let (i, id) = key.split_once('/')?;
                Some((format!("{}/{id}", remap(i.parse().ok()?)?), off.clone()))
            })
            .collect();
        self.parts = moved_parts;
        self.letter_inks = self
            .letter_inks
            .iter()
            .filter_map(|(&i, &c)| Some((remap(i)?, c)))
            .collect();
        self.letters = self
            .letters
            .iter()
            .filter_map(|(&k, v)| Some((remap(k)?, v.clone())))
            .collect();
        self.breaks = self
            .breaks
            .iter()
            .filter_map(|&i| remap(i))
            .filter(|j| allowed.iter().any(|(i, _)| i == j))
            .collect();
        let next_words = words(&new);
        let keep_word = |i: usize| -> Option<usize> {
            let j = remap(i)?;
            let end = self.text[i..]
                .find(char::is_whitespace)
                .map_or(self.text.len(), |n| i + n);
            next_words
                .iter()
                .any(|(start, w)| *start == j && *w == &self.text[i..end])
                .then_some(j)
        };
        self.words = self
            .words
            .iter()
            .filter_map(|(&i, o)| Some((keep_word(i)?, o.clone())))
            .collect();
        self.forms = self
            .forms
            .iter()
            .filter_map(|(&i, o)| Some((keep_word(i)?, o.clone())))
            .collect();
        self.text = new;
    }
}

pub fn atomic_write(path: &Path, data: &[u8]) -> io::Result<()> {
    let mut file = tempfile::NamedTempFile::new_in(path.parent().unwrap_or(Path::new(".")))?;
    file.write_all(data)?;
    file.as_file().sync_all()?;
    file.persist(path)?;
    Ok(())
}

impl Offset {
    pub fn within(&self, bound: f64) -> bool {
        (-bound..=bound).contains(&self.x) && (-bound..=bound).contains(&self.y)
    }
}
pub fn words(text: &str) -> Vec<(usize, &str)> {
    text.split_whitespace()
        .map(|w| ((w.as_ptr() as usize) - (text.as_ptr() as usize), w))
        .collect()
}

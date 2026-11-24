use crate::{
    book::Font,
    document::{self, Document, words},
};
use std::{fs, path::Path};
pub fn change_font(doc: &mut Document, font: Font) {
    if doc.style.font != font {
        doc.style.font = font;
        doc.parts.clear();
        doc.marks.clear();
        doc.forms.clear();
    }
}
pub fn save(path: &Path, doc: &Document) -> Result<(), String> {
    document::atomic_write(path, &serde_json::to_vec(doc).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}
pub fn load(path: &Path) -> Result<Document, String> {
    if fs::metadata(path).map_err(|e| e.to_string())?.len() > 512000 {
        return Err("ملف الأسلوب كبير جدًا".into());
    }
    let doc: Document = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|_| "ملف الأسلوب غير صالح")?;
    doc.valid()
        .then_some(doc)
        .ok_or("قيم الأسلوب غير صالحة".into())
}
pub fn apply(doc: &mut Document, model: &Document) {
    change_font(doc, model.style.font);
    doc.style = model.style.clone();
    let samples = words(&model.text);
    let targets: Vec<_> = words(&doc.text)
        .into_iter()
        .map(|(i, w)| (i, w.to_owned()))
        .collect();
    for (i, w) in targets {
        let Some(&(j, _)) = samples.iter().find(|(_, word)| *word == w) else {
            continue;
        };
        let end = j + w.len();
        doc.letter_inks
            .retain(|&k, _| !(i..i + w.len()).contains(&k));
        for (&k, &color) in model.letter_inks.range(j..end) {
            doc.letter_inks.insert(i + k - j, color);
        }
        for (key, value) in model.marks.iter().chain(model.parts.iter()) {
            if let Some((n, id)) = key.split_once('/')
                && let Ok(n) = n.parse::<usize>()
                && (j..end).contains(&n)
            {
                let map = if id.matches('/').count() == 2 {
                    &mut doc.parts
                } else {
                    &mut doc.marks
                };
                map.insert(format!("{}/{id}", i + n - j), value.clone());
            }
        }
        if let Some(f) = model.forms.get(&j) {
            doc.forms.insert(i, f.clone());
        }
        for (&n, &count) in model.stretches.range(j..end) {
            doc.stretches.insert(i + n - j, count);
        }
        for &n in model.breaks.range(j..end) {
            doc.breaks.insert(i + n - j);
        }
    }
}

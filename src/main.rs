#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
mod backdrop;
mod board;
mod book;
mod document;
mod klx;
mod klx_text;
#[cfg(target_os = "macos")]
mod mac;
mod output;
mod profile;
mod scene;
#[cfg(target_os = "windows")]
mod windows;
fn main() {
    #[cfg(target_os = "macos")]
    mac::run();
    #[cfg(target_os = "windows")]
    windows::run();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    eprintln!("مداد — تتوفر الواجهة الحالية على macOS وWindows");
}

#[cfg(test)]
mod tests {
    use crate::{
        document::{Document, Offset, joins},
        scene::Scene,
    };
    #[test]
    fn mixed_arabic_and_numbers_keep_reading_order() {
        let mut doc = Document::default();
        doc.edit_text("مداد ١٢٣".into());
        let scene = Scene::shape(&doc);
        let numeric: Vec<_> = scene
            .glyphs
            .iter()
            .filter(|g| doc.text[g.cluster..].starts_with(['١', '٢', '٣']))
            .map(|g| g.cluster)
            .collect();
        assert_eq!(numeric.len(), 3);
        assert!(numeric.windows(2).all(|p| p[0] < p[1]));
        doc.edit_text(String::new());
        assert!(Scene::shape(&doc).glyphs.is_empty());
    }
    #[test]
    fn kasra_drag_and_stretch_survive_reopening() {
        let mut doc = Document::default();
        doc.edit_text("بِسْمِ".into());
        let initial = Scene::shape(&doc);
        let mark = initial
            .glyphs
            .iter()
            .find(|g| g.mark.is_some() && doc.text[g.cluster..].starts_with('ِ'))
            .expect("kasra retains original cluster");
        let key = mark.mark.clone().unwrap();
        doc.marks.insert(key.clone(), Offset { x: 7., y: 15. });
        let moved = Scene::shape(&doc);
        let after = moved
            .glyphs
            .iter()
            .find(|g| g.mark.as_ref() == Some(&key))
            .unwrap();
        assert!((after.bounds[0] - mark.bounds[0] - 7.).abs() < 0.001);
        assert!((after.bounds[1] - mark.bounds[1] - 15.).abs() < 0.001);
        let logical = doc.text.clone();
        doc.stretches.insert(joins(&doc.text)[0].0, 2);
        let extended = Scene::shape(&doc);
        assert_ne!(extended.svg(None), moved.svg(None));
        assert!(
            extended
                .glyphs
                .iter()
                .any(|g| g.mark.as_ref() == Some(&key))
        );
        assert_eq!(doc.text, logical);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("لوحة.midad");
        doc.save(&path).unwrap();
        let restored = Document::load(&path).unwrap();
        assert_eq!(restored, doc);
        assert_eq!(extended.svg(None), Scene::shape(&restored).svg(None));
        assert!(!extended.svg(None).contains("<text"));
        assert_ne!(extended.svg(None), extended.svg(Some(&key)));
    }
    #[test]
    fn joins_respect_marks_lam_alef_and_non_joiners() {
        assert_eq!(joins("بِت")[0].0, "بِ".len());
        for text in ["لا", "لَأ", "اب", "دب", "ب ب", "ب\u{200c}ب"] {
            assert!(joins(text).is_empty(), "{text}");
        }
    }
    #[test]
    fn project_validation_and_text_edits() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.midad");
        let mut doc = Document::default();
        doc.edit_text("بِت".into());
        let i = joins(&doc.text)[0].0;
        doc.stretches.insert(i, 1);
        doc.edit_text("م بِت".into());
        assert_eq!(doc.stretches.get(&(i + "م ".len())), Some(&1));
        doc.edit_text("م ب".into());
        assert!(doc.stretches.is_empty());
        doc.stretches.insert(1, 3);
        doc.save(&path).unwrap();
        assert!(Document::load(&path).is_err());
    }
}
#[cfg(test)]
mod studio_tests;

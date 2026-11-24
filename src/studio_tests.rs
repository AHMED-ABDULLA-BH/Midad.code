use crate::{
    board::{Board, Target},
    book::{Book, Font, Layer, Page},
    document::{Document, Offset, joins},
    output, profile,
    scene::{Scene, available_forms},
};
use std::fs;
fn line(text: &str, font: Font, size: f64, y: f64) -> Layer {
    let mut content = Document::default();
    content.edit_text(text.into());
    content.style.font = font;
    content.style.size = size;
    Layer {
        reference: None,
        baseline: None,
        content,
        x: 0.5,
        y,
    }
}
fn specimen() -> Book {
    let mut book = Book::default();
    let first = &mut book.pages[0];
    first.border = true;
    first.lines = vec![
        line("مِداد", Font::Ruqaa, 1.6, 0.40),
        line("جَمَالُ الحَرْفِ العَرَبِيِّ", Font::Amiri, 0.71, 0.70),
    ];
    first.lines[1].content.style.ink = 4;
    first.lines[1].content.style.spacing = 8.;
    let second = Page {
        lines: vec![
            line("فَنُّ الخَطِّ", Font::Kufi, 1.15, 0.38),
            line("حِبْرٌ وَوَرَقٌ", Font::Nastaliq, 0.73, 0.68),
        ],
        paper: 2,
        border: true,
        ..Default::default()
    };
    book.pages.push(second);
    book
}
#[test]
fn two_page_roundtrip_and_exports() {
    let book = specimen();
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("test.midad");
    book.save(&path).unwrap();
    assert_eq!(Book::load(&path).unwrap(), book);
    for ext in ["svg", "pdf", "png"] {
        output::export(&book, 0, ext, &tmp.path().join(format!("test.{ext}"))).unwrap();
    }
    let png = image::open(tmp.path().join("test.png"))
        .unwrap()
        .into_rgb8();
    assert_eq!(png.dimensions(), (2200, 1480));
    let ink_pixels = png.pixels().filter(|p| p.0[0] < 90 && p.0[1] < 120).count();
    assert!(ink_pixels > 500);
    let svg = fs::read_to_string(tmp.path().join("test.svg")).unwrap();
    assert!(!svg.contains("stroke-dasharray"));
    assert!(svg.contains("<path"));
    assert!(
        fs::read(tmp.path().join("test.pdf"))
            .unwrap()
            .starts_with(b"%PDF")
    );
}
#[test]
fn legacy_shape_and_editable_offsets_survive_upgrade() {
    let old =
        r#"{"version":1,"text":"بِسْمِ","stretches":{"4":2},"marks":{"2/96/0":{"x":8.0,"y":6.0}}}"#;
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("old.midad");
    fs::write(&path, old).unwrap();
    let book = Book::load(&path).unwrap();
    assert_eq!(book.version, 2);
    let d = &book.pages[0].lines[0].content;
    assert_eq!(d.stretches[&4], 2);
    assert_eq!(d.marks["2/96/0"].x, 8.);
    book.save(&path).unwrap();
    assert_eq!(book, Book::load(&path).unwrap());
}
#[test]
fn joining_alternatives_and_detached_contours_change_ink() {
    let mut d = Document::default();
    d.edit_text("بت".into());
    let svg = |d: &Document| Scene::shape(d).svg(None);
    let orig = svg(&d);
    let i = joins(&d.text)[0].0;
    d.breaks.insert(i);
    assert_ne!(svg(&d), orig);
    d.breaks.remove(&i);
    assert_eq!(svg(&d), orig);
    let sc = Scene::shape(&d);
    let g = sc.glyphs.iter().find(|g| g.parts.len() > 1).unwrap();
    d.parts
        .insert(g.parts[1].0.clone(), Offset { x: 8., y: 6. });
    assert_ne!(orig, svg(&d));
    assert_eq!(d.text, "بت");
    d.edit_text("نون".into());
    profile::change_font(&mut d, Font::Ruqaa);
    assert!(d.parts.is_empty());
    let choices = available_forms(&d, 0);
    assert!(choices.iter().any(|(tag, _)| *tag == "ss03"));
    let base = svg(&d);
    d.forms.insert(0, "ss03".into());
    assert_ne!(base, svg(&d));
}
#[test]
fn compose_marks_move_with_base_and_word_and_reload_style() {
    let mut d = Document::default();
    d.edit_text("بِسْمِ".into());
    let mark = |d: &Document| {
        Scene::shape(d)
            .glyphs
            .into_iter()
            .find(|g| g.mark.is_some() && g.cluster == 2)
            .unwrap()
    };
    let g = mark(&d);
    d.letters.insert(g.owner, Offset { x: 11., y: 9. });
    let shifted = mark(&d);
    assert!((shifted.x - g.x - 11.).abs() < 1e-6);
    d.marks
        .insert(g.mark.clone().unwrap(), Offset { x: 3., y: 5. });
    let mut target = Document::default();
    target.edit_text("قلم بِسْمِ".into());
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("style.midadstyle");
    profile::save(&path, &d).unwrap();
    profile::apply(&mut target, &profile::load(&path).unwrap());
    assert!(target.valid());
    assert!(
        target
            .marks
            .keys()
            .any(|k| k.starts_with(&format!("{}/", "قلم ".len() + 2)))
    );
    let mut book = specimen();
    book.pages[0].lines[0].content = d;
    book.pages[0].lines[0].content.style.angle = 30.;
    let page = &book.pages[0];
    let board = Board::new(page);
    let layer = &board.lines[0];
    let mark = layer
        .scene
        .glyphs
        .iter()
        .find(|g| g.mark.is_some())
        .unwrap();
    let r = mark.bounds;
    let (x, y) = layer.frame.point((r[0] + r[2]) / 2., (r[1] + r[3]) / 2.);
    assert!(matches!(board.hit(x, y, 2, 0.), Some((0, Target::Mark(_)))));
}
#[test]
#[ignore = "emit local QA files to the chosen directory"]
fn write_local_specimen() {
    let dir = std::path::PathBuf::from(std::env::var_os("MIDAD_QA_DIR").unwrap());
    let b = specimen();
    b.save(&dir.join("لوحة مداد.midad")).unwrap();
    for ext in ["svg", "pdf", "png"] {
        output::export(&b, 0, ext, &dir.join(format!("لوحة مداد.{ext}"))).unwrap();
    }
}

#[test]
fn review_wraps_across_pages_using_native_character_offsets() {
    let mut book = Book::default();
    book.pages[0].lines[0].content.edit_text("مداد 🖋�".into());
    book.pages.push(Page::default());
    book.pages[1].lines[0].content.edit_text("� حبر".into());
    assert_eq!(book.next_unread((0, 0, 0)), Some((0, 0, 7)));
    assert_eq!(book.next_unread((0, 0, 8)), Some((1, 0, 0)));
    assert_eq!(book.next_unread((1, 0, 1)), Some((0, 0, 7)));
    book.pages[0].lines[0].content.edit_text(String::new());
    assert_eq!(book.next_unread((1, 0, 1)), Some((1, 0, 0)));
    book.pages.pop();
    assert_eq!(book.next_unread((0, 0, 0)), None);
}

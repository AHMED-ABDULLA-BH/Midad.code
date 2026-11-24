use crate::document::{self, Document};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

pub const MAX_PAGES: usize = 24;
pub const MAX_LINES: usize = 48;
pub const INKS: [(&str, &str); 6] = [
    ("حبر داكن", "#183b36"),
    ("فحم", "#262727"),
    ("نيلي", "#263f76"),
    ("عنّابي", "#843d48"),
    ("ذهبي", "#92682c"),
    ("زمرد", "#186b55"),
];
pub const PAPERS: [(&str, &str); 3] = [("عاجي", "#fffdf7"), ("أبيض", "#ffffff"), ("رقّ", "#f4ead4")];
#[derive(Clone, Debug, Default, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Font {
    #[default]
    Amiri,
    Ruqaa,
    Kufi,
    Nastaliq,
    Thuluth,
    Farisi,
}
impl Font {
    pub fn available() -> Vec<Self> {
        [
            Self::Amiri,
            Self::Ruqaa,
            Self::Kufi,
            Self::Nastaliq,
            Self::Thuluth,
            Self::Farisi,
        ]
        .into_iter()
        .filter(|f| !f.data().is_empty())
        .collect()
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Amiri => "نسخ · أميري",
            Self::Ruqaa => "رقعة · عارف",
            Self::Kufi => "كوفي · ريم",
            Self::Nastaliq => "نستعليق · نوتو",
            Self::Thuluth => "ثلث · ديوان",
            Self::Farisi => "فارسي",
        }
    }
    pub fn data(self) -> &'static [u8] {
        match self {
            Self::Amiri => include_bytes!("../assets/Amiri-Regular.ttf"),
            Self::Ruqaa => include_bytes!("../assets/arefruqaa/font.ttf"),
            Self::Kufi => include_bytes!("../assets/reemkufi/font.ttf"),
            Self::Thuluth | Self::Farisi => {
                use std::sync::OnceLock;
                static THULUTH: OnceLock<Vec<u8>> = OnceLock::new();
                static FARISI: OnceLock<Vec<u8>> = OnceLock::new();
                let (slot, file) = if self == Self::Thuluth {
                    (&THULUTH, "Diwan Thuluth.ttf")
                } else {
                    (&FARISI, "Farisi.ttf")
                };
                slot.get_or_init(|| {
                    fs::read(Path::new("/System/Library/Fonts/Supplemental").join(file))
                        .ok()
                        .filter(|b| rustybuzz::Face::from_slice(b, 0).is_some())
                        .unwrap_or_default()
                })
            }
            Self::Nastaliq => include_bytes!("../assets/notonastaliqurdu/font.ttf"),
        }
    }
    pub fn forms(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::Amiri => &[
                ("ss01", "خفض نقطة الباء"),
                ("ss02", "تركيب الميم والألف"),
                ("ss05", "الكسرة تحت الشدّة"),
                ("ss06", "حركات لفظ الجلالة اليدوية"),
                ("ss07", "كشيدة مستقيمة"),
            ],
            Self::Ruqaa => &[
                ("ss01", "هاء معقودة"),
                ("ss02", "هاء نهائية مفتوحة"),
                ("ss03", "نون بذيل"),
                ("ss04", "كاف معقودة"),
                ("ss06", "شين بذيل"),
                ("ss07", "تركيب الصاد والطاء"),
                ("ss08", "سكون بديل"),
                ("jalt", "شكل ممدود"),
            ],
            Self::Kufi => &[
                ("cv01", "بديل كوفي ١"),
                ("cv02", "بديل كوفي ٢"),
                ("cv03", "بديل كوفي ٣"),
            ],
            Self::Nastaliq | Self::Thuluth | Self::Farisi => &[],
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Style {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_ink: Option<[u8; 3]>,
    pub font: Font,
    pub ink: usize,
    pub size: f64,
    pub angle: f64,
    pub curve: f64,
    pub spacing: f64,
}
impl Default for Style {
    fn default() -> Self {
        Self {
            custom_ink: None,
            font: Font::Amiri,
            ink: 0,
            size: 1.,
            angle: 0.,
            curve: 0.,
            spacing: 0.,
        }
    }
}
pub fn hex_color([r, g, b]: [u8; 3]) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}
impl Style {
    pub fn color(&self) -> String {
        self.custom_ink
            .map(hex_color)
            .unwrap_or_else(|| INKS[self.ink].1.to_owned())
    }
    pub fn valid(&self) -> bool {
        !self.font.data().is_empty()
            && self.ink < INKS.len()
            && (0.2..=3.).contains(&self.size)
            && (-180. ..=180.).contains(&self.angle)
            && (-180. ..=180.).contains(&self.curve)
            && (-50. ..=120.).contains(&self.spacing)
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KlxAnchor {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ink: Option<[u8; 3]>,
    pub index: usize,
    pub x: f64,
    pub y: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub codes: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape: Option<i32>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct KlxReference {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_style: Option<u8>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub line_codes: Vec<u8>,
    // The owning component records have their own order and may contain signed sentinels.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<i32>,
    pub parts: Vec<KlxAnchor>,
    pub marks: Vec<KlxAnchor>,
}
impl KlxReference {
    fn valid(&self) -> bool {
        self.line_codes.len() <= 256
            && self.attachments.len() <= 4096
            && [&self.parts, &self.marks].into_iter().all(|points| {
                points.len() <= 4096
                    && points.iter().all(|p| {
                        (1..=4096).contains(&p.index)
                            && p.x.abs() <= 100000.
                            && p.y.abs() <= 100000.
                            && p.codes.len() <= 256
                    })
            })
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<KlxReference>,
    #[serde(default)]
    pub baseline: Option<[f64; 4]>,
    pub content: Document,
    pub x: f64,
    pub y: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Picture {
    pub image: crate::backdrop::Backdrop,
    // Signed extents describe reflections about the insertion point.
    pub rect: [f64; 4],
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SourceGuide {
    pub kind: u8,
    pub points: [f64; 4],
}
impl SourceGuide {
    pub const CIRCLE: u8 = 3;
    pub const ELLIPSE: u8 = 4;
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Page {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pictures: Vec<Picture>,
    #[serde(default)]
    pub hide_pictures: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_guides: Vec<SourceGuide>,
    #[serde(default)]
    pub missing_pictures: usize,
    pub width: f64,
    pub height: f64,
    pub paper: usize,
    pub border: bool,
    #[serde(default)]
    pub guides: Vec<(bool, f64)>,
    #[serde(default)]
    pub background: Option<crate::backdrop::Backdrop>,
    pub lines: Vec<Layer>,
}
impl Default for Page {
    fn default() -> Self {
        Self {
            width: 1100.,
            height: 740.,
            paper: 0,
            border: false,
            guides: vec![],
            background: None,
            pictures: vec![],
            hide_pictures: false,
            source_guides: vec![],
            missing_pictures: 0,
            lines: vec![Layer {
                reference: None,
                baseline: None,
                content: Document::default(),
                x: 0.5,
                y: 0.5,
            }],
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Book {
    pub version: u8,
    pub pages: Vec<Page>,
}
impl Default for Book {
    fn default() -> Self {
        Self {
            version: 2,
            pages: vec![Page::default()],
        }
    }
}
impl Book {
    // AppKit text selections use UTF-16 offsets. Wrap once after the last occurrence.
    pub fn next_unread(&self, from: (usize, usize, usize)) -> Option<(usize, usize, usize)> {
        let mut first = None;
        for (p, page) in self.pages.iter().enumerate() {
            for (l, line) in page.lines.iter().enumerate() {
                for (offset, c) in line.content.text.encode_utf16().enumerate() {
                    if c == 0xfffd {
                        let hit = (p, l, offset);
                        first.get_or_insert(hit);
                        if hit >= from {
                            return Some(hit);
                        }
                    }
                }
            }
        }
        first
    }
    pub fn valid(&self) -> bool {
        self.version == 2
            && (1..=MAX_PAGES).contains(&self.pages.len())
            && self.pages.iter().all(|p| {
                (100. ..=20000.).contains(&p.width)
                    && (100. ..=20000.).contains(&p.height)
                    && p.pictures.len() <= 128
                    && p.source_guides.len() <= 256
                    && p.pictures
                        .iter()
                        .all(|pic| pic.rect.iter().all(|n| n.abs() <= 100000.))
                    && p.source_guides
                        .iter()
                        .all(|g| g.points.iter().all(|n| n.abs() <= 100000.))
                    && p.paper < PAPERS.len()
                    && p.guides.len() <= 128
                    && p.guides.iter().all(|(_, n)| (0. ..=1.).contains(n))
                    && (1..=MAX_LINES).contains(&p.lines.len())
                    && p.lines.iter().all(|l| {
                        (-64. ..=64.).contains(&l.x)
                            && (-64. ..=64.).contains(&l.y)
                            && l.baseline
                                .is_none_or(|r| r.iter().all(|n| n.abs() <= 100000.))
                            && l.reference.as_ref().is_none_or(KlxReference::valid)
                            && l.content.valid()
                    })
            })
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        if fs::metadata(path).map_err(|e| e.to_string())?.len() > 32_000_000 {
            return Err("ملف المشروع كبير جدًا".into());
        }
        let bytes = fs::read(path).map_err(|e| e.to_string())?;
        let value: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|_| "ملف مداد غير صالح")?;
        let mut book = match value["version"].as_u64() {
            Some(1) => {
                let mut old: Document =
                    serde_json::from_value(value).map_err(|_| "تعذرت قراءة السطر القديم")?;
                if old.style.font.data().is_empty() {
                    crate::profile::change_font(&mut old, Font::Amiri);
                }
                if !old.valid() {
                    return Err("بيانات السطر غير صالحة".into());
                }
                let scene = crate::scene::Scene::shape(&old);
                let mut b = Book::default();
                b.pages[0].width = scene.width;
                b.pages[0].height = scene.height;
                b.pages[0].lines[0].content = old;
                b
            }
            Some(2) => serde_json::from_value(value).map_err(|_| "بنية المشروع غير صالحة")?,
            _ => return Err("إصدار المشروع غير مدعوم".into()),
        };
        for layer in book.pages.iter_mut().flat_map(|p| &mut p.lines) {
            if layer.content.style.font.data().is_empty() {
                crate::profile::change_font(&mut layer.content, Font::Amiri);
            }
        }
        book.valid()
            .then_some(book)
            .ok_or_else(|| "قيم اللوحة خارج النطاق".into())
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if !self.valid() {
            return Err("تعذّر حفظ قيم اللوحة".into());
        }
        let bytes = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        if bytes.len() > 32_000_000 {
            return Err("حجم المشروع يتجاوز ٣٢ ميغابايت".into());
        }
        document::atomic_write(path, &bytes).map_err(|e| e.to_string())
    }
}

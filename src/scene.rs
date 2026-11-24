use crate::document::Document;
use rustybuzz::{
    BufferClusterLevel, Direction, Face, UnicodeBuffer,
    ttf_parser::{GlyphId, OutlineBuilder, gdef::GlyphClass},
};
use std::{collections::HashMap, fmt::Write};

#[cfg(test)]
pub const PAPER: &str = "#fffdf7";
#[cfg(test)]
pub const INK: &str = "#183b36";

struct Outline<'a> {
    path: String,
    parts: Vec<(String, [f64; 4])>,
    key: &'a str,
    doc: &'a Document,
    scale: f64,
    offset: (f32, f32),
}
impl<'a> Outline<'a> {
    fn new(key: &'a str, doc: &'a Document, scale: f64) -> Self {
        Self {
            path: String::new(),
            parts: vec![],
            key,
            doc,
            scale,
            offset: (0., 0.),
        }
    }
    fn point(&mut self, x: f32, y: f32) -> (f32, f32) {
        let (x, y) = (x + self.offset.0, y + self.offset.1);
        if let Some((_, r)) = self.parts.last_mut() {
            r[0] = r[0].min(x.into());
            r[1] = r[1].min(y.into());
            r[2] = r[2].max(x.into());
            r[3] = r[3].max(y.into());
        }
        (x, y)
    }
}
impl OutlineBuilder for Outline<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        let key = format!("{}/{}", self.key, self.parts.len());
        self.offset = self.doc.parts.get(&key).map_or((0., 0.), |o| {
            ((o.x / self.scale) as f32, (-o.y / self.scale) as f32)
        });
        self.parts.push((
            key,
            [
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ],
        ));
        let (x, y) = self.point(x, y);
        write!(self.path, "M{x} {y}").unwrap();
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.point(x, y);
        write!(self.path, "L{x} {y}").unwrap();
    }
    fn quad_to(&mut self, x: f32, y: f32, u: f32, v: f32) {
        let (x, y) = self.point(x, y);
        let (u, v) = self.point(u, v);
        write!(self.path, "Q{x} {y} {u} {v}").unwrap();
    }
    fn curve_to(&mut self, x: f32, y: f32, u: f32, v: f32, a: f32, b: f32) {
        let (x, y) = self.point(x, y);
        let (u, v) = self.point(u, v);
        let (a, b) = self.point(a, b);
        write!(self.path, "C{x} {y} {u} {v} {a} {b}").unwrap();
    }
    fn close(&mut self) {
        self.path.push('Z');
    }
}
pub struct Glyph {
    pub path: String,
    pub x: f64,
    pub y: f64,
    pub mark: Option<String>,
    pub bounds: [f64; 4],
    pub cluster: usize,
    pub word: usize,
    pub owner: usize,
    pub parts: Vec<(String, [f64; 4])>,
}
pub struct Scene {
    pub width: f64,
    pub height: f64,
    pub scale: f64,
    pub glyphs: Vec<Glyph>,
}
impl Scene {
    pub fn shape(doc: &Document) -> Self {
        let face = Face::from_slice(doc.style.font.data(), 0).expect("embedded Amiri");
        let scale = 112.0 / f64::from(face.units_per_em());
        let mut items = Vec::new();
        let mut pen = 0.;
        let words = crate::document::words(&doc.text);
        let features: Vec<_> = words
            .iter()
            .filter_map(|(i, w)| {
                let tag = doc.forms.get(i)?;
                let tag: &[u8; 4] = tag.as_bytes().try_into().ok()?;
                Some(rustybuzz::Feature::new(
                    rustybuzz::ttf_parser::Tag::from_bytes(tag),
                    1,
                    *i..*i + w.len(),
                ))
            })
            .collect();
        let mut ordinals = HashMap::new();
        let bidi = unicode_bidi::BidiInfo::new(&doc.text, Some(unicode_bidi::Level::rtl()));
        for para in &bidi.paragraphs {
            let (levels, runs) = bidi.visual_runs(para, para.range.clone());
            for range in runs {
                let mut buffer = UnicodeBuffer::new();
                buffer.set_cluster_level(BufferClusterLevel::Characters);
                for (i, c) in doc.text[range.clone()].char_indices() {
                    let i = range.start + i;
                    if doc.breaks.contains(&i) {
                        buffer.add('‌', i as u32);
                    } else {
                        for _ in 0..doc.stretches.get(&i).copied().unwrap_or(0) {
                            buffer.add('ـ', i as u32);
                        }
                    }
                    buffer.add(c, i as u32);
                }
                buffer.set_direction(if levels[range.start].is_rtl() {
                    Direction::RightToLeft
                } else {
                    Direction::LeftToRight
                });
                buffer.guess_segment_properties();
                let shaped = rustybuzz::shape(&face, &features, buffer);
                for (info, pos) in shaped.glyph_infos().iter().zip(shaped.glyph_positions()) {
                    let id = GlyphId(info.glyph_id as u16);
                    let mut x = pen + f64::from(pos.x_offset) * scale;
                    let mut y = -f64::from(pos.y_offset) * scale;
                    pen += f64::from(pos.x_advance) * scale;
                    let cluster = info.cluster as usize;
                    let ordinal = ordinals.entry((cluster, id.0)).or_insert(0);
                    let identity = format!("{cluster}/{}/{ordinal}", id.0);
                    *ordinal += 1;
                    let mut outline = Outline::new(&identity, doc, scale);
                    let Some(bb) = face.outline_glyph(id, &mut outline) else {
                        continue;
                    };
                    let cluster = info.cluster as usize;
                    let mark = if face.tables().gdef.and_then(|t| t.glyph_class(id))
                        == Some(GlyphClass::Mark)
                    {
                        let key = identity.clone();
                        if let Some(off) = doc.marks.get(&key) {
                            x += off.x;
                            y += off.y;
                        }
                        Some(key)
                    } else {
                        None
                    };
                    let owner = doc.text[..cluster]
                        .char_indices()
                        .rev()
                        .find(|(_, c)| {
                            unicode_joining_type::get_joining_type(*c)
                                != unicode_joining_type::JoiningType::Transparent
                        })
                        .map_or(cluster, |(i, _)| i);
                    let owner = if mark.is_some()
                        && doc.text[cluster..].chars().next().is_some_and(|c| {
                            unicode_joining_type::get_joining_type(c)
                                == unicode_joining_type::JoiningType::Transparent
                        }) {
                        owner
                    } else {
                        cluster
                    };
                    items.push(Glyph {
                        owner,
                        word: words
                            .iter()
                            .rev()
                            .find(|(i, _)| *i <= cluster)
                            .map_or(0, |(i, _)| *i),
                        parts: outline
                            .parts
                            .into_iter()
                            .map(|(k, r)| {
                                (
                                    k,
                                    [
                                        x + r[0] * scale,
                                        y - r[3] * scale,
                                        x + r[2] * scale,
                                        y - r[1] * scale,
                                    ],
                                )
                            })
                            .collect(),
                        path: outline.path,
                        x,
                        y,
                        cluster,
                        mark,
                        bounds: [
                            x + f64::from(bb.x_min) * scale,
                            y - f64::from(bb.y_max) * scale,
                            x + f64::from(bb.x_max) * scale,
                            y - f64::from(bb.y_min) * scale,
                        ],
                    });
                }
            }
        }
        let bases: std::collections::BTreeSet<_> = items
            .iter()
            .filter(|g| g.mark.is_none())
            .map(|g| (g.word, g.cluster))
            .collect();
        for g in &mut items {
            if g.mark.is_some()
                && let Some(&(_, i)) = bases.range((g.word, 0)..=(g.word, g.owner)).next_back()
            {
                g.owner = i;
            }
        }
        // Frame by the unadjusted ink so dragging a mark never pans the page.
        let (mut left, mut right, mut top, mut bottom) = (0f64, pen, 0f64, 0f64);
        for g in &items {
            let offset = g.mark.as_ref().and_then(|k| doc.marks.get(k));
            let (dx, dy) = offset.map_or((0., 0.), |o| (o.x, o.y));
            left = left.min(g.bounds[0] - dx);
            right = right.max(g.bounds[2] - dx);
            top = top.min(g.bounds[1] - dy);
            bottom = bottom.max(g.bounds[3] - dy);
        }
        let width = (right - left + 160.).max(760.);
        let height = (bottom - top + 144.).max(360.);
        let dx = (width - left - right) / 2.;
        let dy = (height - top - bottom) / 2.;
        for g in &mut items {
            for (_, r) in &mut g.parts {
                r[0] += dx;
                r[2] += dx;
                r[1] += dy;
                r[3] += dy;
            }
            let o = doc.letters.get(&g.owner).cloned().unwrap_or_default();
            for (_, r) in &mut g.parts {
                r[0] += o.x;
                r[2] += o.x;
                r[1] += o.y;
                r[3] += o.y;
            }
            let (dx, dy) = (dx + o.x, dy + o.y);
            g.x += dx;
            g.y += dy;
            g.bounds[0] += dx;
            g.bounds[2] += dx;
            g.bounds[1] += dy;
            g.bounds[3] += dy;
        }
        Self {
            width,
            height,
            scale,
            glyphs: items,
        }
    }
    #[cfg(test)]
    pub fn svg(&self, selected: Option<&str>) -> String {
        let (width, height) = (self.width, self.height);
        let mut svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}"><rect width="{width}" height="{height}" fill="{PAPER}"/>"#,
        );
        for g in &self.glyphs {
            write!(
                svg,
                r#"<path fill="{INK}" transform="translate({} {}) scale({} {})" d="{}"/>"#,
                g.x, g.y, self.scale, -self.scale, g.path
            )
            .unwrap();
        }
        if let Some(g) =
            selected.and_then(|k| self.glyphs.iter().find(|g| g.mark.as_deref() == Some(k)))
        {
            let [x, y, r, b] = g.bounds;
            write!(svg,r##"<rect x="{}" y="{}" width="{}" height="{}" rx="3" fill="none" stroke="#b76935" stroke-width="1.4"/>"##,x-3.,y-3.,r-x+6.,b-y+6.).unwrap();
        }
        svg.push_str("</svg>");
        svg
    }
}

// Test each advertised OpenType feature on the selected word. Several faces have
// features that affect only a single letter or pair. Hide the others in the palette.
pub fn available_forms(doc: &Document, start: usize) -> Vec<(&'static str, &'static str)> {
    let Some((_, word)) = crate::document::words(&doc.text)
        .into_iter()
        .find(|(k, _)| *k == start)
    else {
        return vec![];
    };
    let mut sample = Document::default();
    sample.edit_text(word.to_owned());
    sample.style.font = doc.style.font;
    sample.stretches = doc
        .stretches
        .range(start..start + word.len())
        .map(|(i, v)| (i - start, *v))
        .collect();
    sample.breaks = doc
        .breaks
        .range(start..start + word.len())
        .map(|i| i - start)
        .collect();
    let signature = |line: &Document| {
        Scene::shape(line)
            .glyphs
            .into_iter()
            .map(|g| (g.cluster, g.path, g.x.to_bits(), g.y.to_bits()))
            .collect::<Vec<_>>()
    };
    let base = signature(&sample);
    doc.style
        .font
        .forms()
        .iter()
        .copied()
        .filter(|(tag, _)| {
            sample.forms.insert(0, tag.to_string());
            signature(&sample) != base || doc.forms.get(&start).is_some_and(|v| v == tag)
        })
        .collect()
}

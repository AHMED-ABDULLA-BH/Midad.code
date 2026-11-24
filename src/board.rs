use crate::{
    book::{PAPERS, Page, SourceGuide, hex_color},
    document::Offset,
    scene::Scene,
};
use std::{collections::BTreeMap, fmt::Write};

#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    Line,
    Word(usize),
    Letter(usize),
    Mark(String),
    Part(String),
}
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub x: f64,
    pub y: f64,
    pub scale: f64,
    pub angle: f64,
}
impl Frame {
    pub(crate) fn for_line(
        line: &crate::book::Layer,
        scene: &Scene,
        width: f64,
        height: f64,
    ) -> Self {
        let mut frame = Self {
            x: line.x * width,
            y: line.y * height,
            scale: line.content.style.size * (width / scene.width).min(1.),
            angle: line.content.style.angle,
        };
        (frame.x, frame.y) = frame.point(-scene.width / 2., -scene.height / 2.);
        frame
    }
    pub fn point(self, x: f64, y: f64) -> (f64, f64) {
        let (s, c) = self.angle.to_radians().sin_cos();
        (
            self.x + self.scale * (c * x - s * y),
            self.y + self.scale * (s * x + c * y),
        )
    }
    pub fn inverse_delta(self, x: f64, y: f64) -> (f64, f64) {
        let (s, c) = self.angle.to_radians().sin_cos();
        ((c * x + s * y) / self.scale, (-s * x + c * y) / self.scale)
    }
    fn local(self, x: f64, y: f64) -> (f64, f64) {
        self.inverse_delta(x - self.x, y - self.y)
    }
    fn svg(self) -> String {
        format!(
            "translate({} {}) rotate({}) scale({})",
            self.x, self.y, self.angle, self.scale
        )
    }
}
impl SourceGuide {
    // KLX stores opposite corners in page units, including coordinates beyond the paper.
    pub(crate) fn svg_element(&self) -> Option<String> {
        let [x, y, xx, yy] = self.points;
        if self.kind <= 2 {
            return Some(format!(r#"<path d="M{x} {y}L{xx} {yy}"/>"#));
        }
        let (cx, cy) = ((x + xx) / 2., (y + yy) / 2.);
        let (rx, ry) = ((xx - x).abs() / 2., (yy - y).abs() / 2.);
        if rx == 0. || ry == 0. {
            return None;
        }
        match self.kind {
            Self::CIRCLE => Some(format!(
                r#"<circle cx="{cx}" cy="{cy}" r="{}"/>"#,
                rx.min(ry)
            )),
            Self::ELLIPSE => Some(format!(
                r#"<ellipse cx="{cx}" cy="{cy}" rx="{rx}" ry="{ry}"/>"#
            )),
            _ => None,
        }
    }
}
pub struct Arrangement {
    pub scene: Scene,
    pub frame: Frame,
    words: BTreeMap<usize, Offset>,
}
pub struct Board {
    pub lines: Vec<Arrangement>,
}
impl Board {
    pub fn new(page: &Page) -> Self {
        let lines = page
            .lines
            .iter()
            .map(|l| {
                let scene = Scene::shape(&l.content);
                let style = &l.content.style;
                let frame = Frame::for_line(l, &scene, page.width, page.height);
                let mut centers: BTreeMap<usize, (f64, f64)> = BTreeMap::new();
                for g in &scene.glyphs {
                    let e = centers
                        .entry(g.word)
                        .or_insert((f64::INFINITY, f64::NEG_INFINITY));
                    e.0 = e.0.min(g.bounds[0]);
                    e.1 = e.1.max(g.bounds[2]);
                }
                let count = centers.len() as f64;
                let words = centers
                    .into_iter()
                    .enumerate()
                    .map(|(i, (key, (lo, hi)))| {
                        let mut offset = l.content.words.get(&key).cloned().unwrap_or_default();
                        let t = ((lo + hi) / scene.width - 1.).clamp(-1., 1.);
                        offset.y -= style.curve * (1. - t * t);
                        offset.x += ((count - 1.) / 2. - i as f64) * style.spacing;
                        (key, offset)
                    })
                    .collect();
                Arrangement {
                    scene,
                    frame,
                    words,
                }
            })
            .collect();
        Self { lines }
    }
    pub fn svg(
        &self,
        page: &Page,
        selection: Option<(usize, &Target)>,
        guide_line: Option<usize>,
    ) -> String {
        let (w, h) = (page.width, page.height);
        let mut out = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{w}" height="{h}" viewBox="0 0 {w} {h}"><rect width="{w}" height="{h}" fill="{}"/>"#,
            PAPERS[page.paper].1
        );
        if let Some(bg) = &page.background {
            write!(out,r#"<image width="{w}" height="{h}" preserveAspectRatio="xMidYMid meet" xlink:href="data:image/png;base64,{}"/>"#,bg.0).unwrap();
        }
        if !page.hide_pictures {
            for pic in &page.pictures {
                let [x, y, pw, ph] = pic.rect;
                if pw == 0. || ph == 0. {
                    continue;
                }
                write!(out,r#"<image transform="translate({x} {y}) scale({} {})" width="{}" height="{}" preserveAspectRatio="none" xlink:href="data:image/png;base64,{}"/>"#,pw.signum(),ph.signum(),pw.abs(),ph.abs(),pic.image.0).unwrap();
            }
        }
        if page.border {
            for m in [18., 24.] {
                write!(out,r##"<rect x="{m}" y="{m}" width="{}" height="{}" fill="none" stroke="#a1804f" stroke-width="{}"/>"##,w-2.*m,h-2.*m,if m==18.{2.}else{0.8}).unwrap();
            }
        }
        if guide_line.is_some() {
            if !page.source_guides.is_empty() {
                out.push_str(
                    r##"<g id="klx-guides" stroke="#648d82" stroke-dasharray="3 4" fill="none">"##,
                );
                for element in page
                    .source_guides
                    .iter()
                    .filter_map(SourceGuide::svg_element)
                {
                    out.push_str(&element);
                }
                out.push_str("</g>");
            }
            for &(vertical, fraction) in &page.guides {
                let d = if vertical {
                    format!("M{} 0V{h}", w * fraction)
                } else {
                    format!("M0 {}H{w}", h * fraction)
                };
                write!(
                    out,
                    r##"<path d="{d}" stroke="#648d82" stroke-dasharray="3 4" fill="none"/>"##
                )
                .unwrap();
            }
            write!(out,r##"<path d="M0 {}H{w} M{} 0V{h}" stroke="#8daca4" stroke-width="1" stroke-dasharray="6 5" fill="none"/>"##,h/2.,w/2.).unwrap();
        }
        for (i, line) in self.lines.iter().enumerate() {
            let scene = &line.scene;
            let layer = &page.lines[i];
            if (guide_line.is_some() || (selection.is_some() && layer.content.text.is_empty()))
                && let Some([x, y, xx, yy]) = layer.baseline
            {
                let (cx, cy) = (layer.x * w, layer.y * h);
                write!(out,r##"<path d="M{} {}L{} {}" stroke="#648d82" stroke-width="2" stroke-dasharray="8 4"/>"##,cx+x,cy+y,cx+xx,cy+yy).unwrap();
            }
            if guide_line == Some(i)
                && let Some(source) = &layer.reference
            {
                let (cx, cy) = (layer.x * w, layer.y * h);
                write!(
                    out,
                    r#"<g transform="translate({cx} {cy})" fill="none" stroke-width="1.2">"#
                )
                .unwrap();
                for part in &source.parts {
                    write!(
                        out,
                        r##"<path d="M{} {}h6m-3 -3v6" stroke="#4d8991"/>"##,
                        part.x - 3.,
                        part.y
                    )
                    .unwrap();
                }
                for mark in &source.marks {
                    write!(
                        out,
                        r##"<circle cx="{}" cy="{}" r="1.8" stroke="#a07943"/>"##,
                        mark.x, mark.y
                    )
                    .unwrap();
                }
                out.push_str("</g>");
            }
            write!(
                out,
                r#"<g transform="{}" fill="{}">"#,
                line.frame.svg(),
                layer.content.style.color()
            )
            .unwrap();
            for g in &scene.glyphs {
                let o = &line.words[&g.word];
                let ink = layer
                    .content
                    .letter_inks
                    .get(&g.cluster)
                    .or_else(|| layer.content.letter_inks.get(&g.owner))
                    .map(|&c| format!(" fill=\"{}\"", hex_color(c)))
                    .unwrap_or_default();
                write!(
                    out,
                    r#"<path{ink} transform="translate({} {}) scale({} {})" d="{}"/>"#,
                    g.x + o.x,
                    g.y + o.y,
                    scene.scale,
                    -scene.scale,
                    g.path
                )
                .unwrap();
            }
            if let Some((_, target)) = selection.filter(|(id, _)| *id == i) {
                let mut r = [
                    f64::INFINITY,
                    f64::INFINITY,
                    f64::NEG_INFINITY,
                    f64::NEG_INFINITY,
                ];
                for g in &scene.glyphs {
                    let o = &line.words[&g.word];
                    let bounds = match target {
                        Target::Letter(k) if *k == g.owner => Some(g.bounds),
                        Target::Line => Some(g.bounds),
                        Target::Word(k) if *k == g.word => Some(g.bounds),
                        Target::Mark(k) if g.mark.as_ref() == Some(k) => Some(g.bounds),
                        Target::Part(k) => g.parts.iter().find(|(id, _)| id == k).map(|(_, r)| *r),
                        _ => None,
                    };
                    if let Some([l, t, b, h]) = bounds {
                        r[0] = r[0].min(l + o.x);
                        r[1] = r[1].min(t + o.y);
                        r[2] = r[2].max(b + o.x);
                        r[3] = r[3].max(h + o.y);
                    }
                }
                if !r[0].is_finite() {
                    r = [
                        scene.width / 2. - 40.,
                        scene.height / 2. - 16.,
                        scene.width / 2. + 40.,
                        scene.height / 2. + 16.,
                    ];
                }
                write!(out,r##"<rect x="{}" y="{}" width="{}" height="{}" rx="3" fill="none" stroke="#ad673c" stroke-width="1.4" stroke-dasharray="5 4"/>"##,r[0]-4.,r[1]-4.,r[2]-r[0]+8.,r[3]-r[1]+8.).unwrap();
            }
            out.push_str("</g>");
        }
        out.push_str("</svg>");
        out
    }
    pub fn hit(&self, x: f64, y: f64, mode: usize, slop: f64) -> Option<(usize, Target)> {
        for (i, line) in self.lines.iter().enumerate().rev() {
            let (x, y) = line.frame.local(x, y);
            let margin = slop / line.frame.scale;
            let mut best = None;
            for g in &line.scene.glyphs {
                let o = &line.words[&g.word];
                let (x, y) = (x - o.x, y - o.y);
                let mut test = |r: [f64; 4], target: Target| {
                    if x >= r[0] - margin
                        && x <= r[2] + margin
                        && y >= r[1] - margin
                        && y <= r[3] + margin
                    {
                        let score = (r[2] - r[0]) * (r[3] - r[1]);
                        if best.as_ref().is_none_or(|(s, _)| score < *s) {
                            best = Some((score, target));
                        }
                    }
                };
                match mode {
                    0 => test(g.bounds, Target::Line),
                    1 => test(g.bounds, Target::Word(g.word)),
                    2 => {
                        if let Some(mark) = &g.mark {
                            test(g.bounds, Target::Mark(mark.clone()))
                        }
                    }
                    4 => test(g.bounds, Target::Letter(g.owner)),
                    3 => {
                        for (k, r) in &g.parts {
                            test(*r, Target::Part(k.clone()))
                        }
                    }
                    _ => {}
                }
            }
            if let Some((_, target)) = best {
                return Some((i, target));
            }
        }
        None
    }
}

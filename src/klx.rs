// Read the documented PropertyBag record chain and recover logical text and source anchor positions.
// Nested records must exactly consume both declared sizes. Only the outer bag may have a stale tail.
use crate::book::{
    Book, Font, KlxAnchor, KlxReference, Layer, MAX_LINES, MAX_PAGES, Page, Picture, SourceGuide,
};
use crate::{
    board::Board,
    document::{self, Document, Offset},
    klx_text,
};
use std::{collections::BTreeMap, fs, path::Path};
type Result<T> = std::result::Result<T, String>;
fn take(b: &[u8], i: usize, n: usize) -> Result<&[u8]> {
    b.get(i..i.checked_add(n).ok_or("حجم غير صالح")?)
        .ok_or("ملف KLX مبتور".into())
}
fn u16at(b: &[u8], i: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(take(b, i, 2)?.try_into().unwrap()))
}
fn u32at(b: &[u8], i: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(take(b, i, 4)?.try_into().unwrap()))
}
#[derive(Default)]
struct Bag {
    pictures: BTreeMap<usize, Vec<u8>>,
    attachments: BTreeMap<String, Vec<i32>>,
    text: BTreeMap<String, Vec<u8>>,
    nums: BTreeMap<String, f64>,
    pages: BTreeMap<usize, Bag>,
}
fn bag(array: &[u8], depth: usize) -> Result<Bag> {
    if depth > 6 || u16at(array, 0)? != 1 || u32at(array, 6)? != 0 {
        return Err("ترويسة KLX غير صالحة".into());
    }
    let size = u32at(array, 2)? as usize;
    let b = take(array, 10, size)?;
    if depth > 0 && array.len() != size + 10
        || u32at(b, 0)? != 0xb293
        || u32at(b, 4)? as usize != size
    {
        return Err("طول حاوية KLX غير متطابق".into());
    }
    let mut data = Bag::default();
    let mut i = 8;
    loop {
        let vt = u16at(b, i)?;
        let chars = u16at(b, i + 2)? as usize;
        let link = u32at(b, i + 8)? as i32;
        let target = (i as i64) + i64::from(link);
        let last = target == 8;
        let next = if last {
            size
        } else {
            usize::try_from(target).map_err(|_| "رابط KLX غير صالح")?
        };
        if next <= i || next > size {
            return Err("سلسلة سجلات KLX غير صالحة".into());
        }
        let body = take(b, i + 12, next.checked_sub(i + 12).ok_or("سجل قصير")?)?;
        let name = String::from_utf16(
            &take(body, 0, chars * 2)?
                .chunks_exact(2)
                .map(|v| u16::from_le_bytes([v[0], v[1]]))
                .collect::<Vec<_>>(),
        )
        .map_err(|_| "اسم حقل تالف")?;
        let v = &body[chars * 2..];
        let mut num = None;
        let text_field = ["ltextstr0_", "attrawstr0_", "punrawstr0_"]
            .iter()
            .any(|p| name.starts_with(p));
        let used = match vt {
            // Older files use VARIANT_BOOL for horizontal/vertical, with -1 for true.
            11 => {
                num = Some(if u16at(v, 0)? == 0 { 0. } else { 1. });
                2
            }
            2 => {
                num = Some(u16at(v, 0)? as i16 as f64);
                2
            }
            3 => {
                num = Some(u32at(v, 0)? as i32 as f64);
                4
            }
            4 => {
                num = Some(f32::from_bits(u32at(v, 0)?) as f64);
                4
            }
            5 | 7 => {
                num = Some(f64::from_le_bytes(take(v, 0, 8)?.try_into().unwrap()));
                8
            }
            6 => 8,
            17 => {
                num = Some(*v.first().ok_or("سجل قصير")? as f64);
                1
            }
            8 => {
                let n = 2 * (u32at(v, 0)? as usize);
                if text_field {
                    data.text
                        .insert(name.clone(), klx_text::unpack(take(v, 4, n)?)?);
                }
                4 + n
            }
            9 if !last => {
                // Serialized StdPicture, followed by a length-prefixed embedded file.
                const HEADER: &[u8] = &[
                    4, 82, 227, 11, 145, 143, 206, 17, 157, 227, 0, 170, 0, 75, 184, 81, 108, 116,
                    0, 0,
                ];
                if let Some(id) = name
                    .strip_prefix("pict0_")
                    .and_then(|s| s.parse::<usize>().ok())
                    && (1..=128).contains(&id)
                    && v.starts_with(HEADER)
                {
                    let length = u32at(v, 20)? as usize;
                    let bytes = take(v, 24, length)?;
                    data.pictures.insert(id, bytes.to_vec());
                    24 + length
                } else {
                    v.len()
                }
            }
            0x2011 if !last => {
                if u16at(v, 0)? != 1 || u32at(v, 6)? != 0 {
                    return Err("مصفوفة KLX تالفة".into());
                }
                let n = u32at(v, 2)? as usize;
                let array = take(v, 0, 10 + n)?;
                if text_field {
                    data.text
                        .insert(name.clone(), klx_text::unpack(&array[10..])?);
                } else if n >= 8 && u32at(array, 10)? == 0xb293 {
                    let nested = bag(array, depth + 1)?;
                    if let Some(index) = name
                        .strip_prefix("page")
                        .and_then(|n| n.parse::<usize>().ok())
                    {
                        if index == 0
                            || index > MAX_PAGES
                            || data.pages.insert(index, nested).is_some()
                        {
                            return Err("ترقيم الصفحات غير مدعوم".into());
                        }
                    }
                }
                10 + n
            }
            _ => return Err(format!("نوع سجل KLX غير مدعوم ({vt})")),
        };
        if used > v.len() || v.len() - used > 3 || v[used..].iter().any(|&c| c != 0) {
            return Err("طول سجل KLX غير صالح".into());
        }
        if vt == 3 && name.starts_with("attachedto0_") {
            let values = data.attachments.entry(name.clone()).or_default();
            if values.len() >= 4096 {
                return Err("عدد روابط KLX خارج النطاق".into());
            }
            values.push(u32at(v, 0)? as i32);
        }
        if let Some(n) = num {
            if [
                "glisvertical0_",
                "glleft0_",
                "gltop0_",
                "glright0_",
                "glbottom0_",
                "guidelinecount0",
                "picturecount0",
                "pictx0_",
                "picty0_",
                "pictw0_",
                "picth0_",
                "transparent0_",
                "lfntstyle0_",
                "lfntcolor0_",
                "attfntcolor0_",
                "punfntcolor0_",
                "pgwidth",
                "pgheight",
                "linecount",
                "lx0_",
                "ly0_",
                "lxx0_",
                "lyy0_",
                "lfntsize0_",
                "ltextcount0_",
                "lpunccount0_",
                "attshapenum0_",
                "punshapenum0_",
                "attx0_",
                "atty0_",
                "punx0_",
                "puny0_",
            ]
            .iter()
            .any(|prefix| name.starts_with(prefix))
            {
                data.nums.insert(name, n);
            }
        }
        if last {
            break;
        }
        i = next;
    }
    Ok(data)
}
fn ink(value: f64) -> Option<[u8; 3]> {
    if !(0. ..=16777215.).contains(&value) || value.fract() != 0. {
        return None;
    }
    let [r, g, b, _] = (value as u32).to_le_bytes();
    Some([r, g, b])
}
impl Bag {
    fn anchors(
        &self,
        line: usize,
        prefix: &str,
        count: f64,
        center: (f64, f64),
    ) -> Result<Vec<KlxAnchor>> {
        if count.fract() != 0. || !(0. ..=4096.).contains(&count) {
            return Err("عدد عناصر سطر KLX غير مدعوم".into());
        }
        (1..=count as usize)
            .map(|index| {
                let key = |property: &str| format!("{prefix}{property}0_{line}_{index}");
                let coord = |axis: &str| {
                    self.nums
                        .get(&key(axis))
                        .copied()
                        .ok_or("إحداثيات عنصر KLX ناقصة")
                };
                Ok(KlxAnchor {
                    index,
                    ink: self.nums.get(&key("fntcolor")).copied().and_then(ink),
                    shape: self.nums.get(&key("shapenum")).map(|&n| n as i32),
                    x: (coord("x")? - center.0) / 15.,
                    y: (coord("y")? - center.1) / 15.,
                    codes: self.text.get(&key("rawstr")).cloned().unwrap_or_default(),
                })
            })
            .collect()
    }
    fn page(&self, image_budget: &mut usize) -> Result<Page> {
        let get = |k: &str| {
            self.nums
                .get(k)
                .copied()
                .filter(|n| n.is_finite())
                .ok_or_else(|| format!("حقل التخطيط غير متوفر ({k})"))
        };
        let (w, h) = (get("pgwidth0")?, get("pgheight0")?);
        let n = get("linecount0")?;
        if w <= 0. || h <= 0. || n.fract() != 0. || !(0. ..=MAX_LINES as f64).contains(&n) {
            return Err("أبعاد الصفحة أو عدد الأسطر غير مدعوم".into());
        }
        let mut page = Page {
            width: w / 15.,
            height: h / 15.,
            ..Page::default()
        };
        let count = |key: &str, limit: usize| -> Result<usize> {
            let n = self.nums.get(key).copied().unwrap_or(0.);
            if n.fract() != 0. || !(0. ..=limit as f64).contains(&n) {
                return Err("عدد عناصر الصفحة خارج النطاق".into());
            }
            Ok(n as usize)
        };
        for i in 1..=count("guidelinecount0", 256)? {
            let v = |k| get(&format!("{k}0_{i}"));
            let kind = v("glisvertical")?;
            if kind.fract() != 0. || !(0. ..=255.).contains(&kind) {
                return Err("رقم نوع دليل KLX غير صالح".into());
            }
            page.source_guides.push(SourceGuide {
                kind: kind as u8,
                points: [v("glleft")?, v("gltop")?, v("glright")?, v("glbottom")?],
            });
        }
        for i in 1..=count("picturecount0", 128)? {
            let v = |k| get(&format!("{k}0_{i}"));
            let rect = [v("pictx")?, v("picty")?, v("pictw")?, v("picth")?].map(|n| n / 15.);
            let Some(bytes) = self.pictures.get(&i) else {
                page.missing_pictures += 1;
                continue;
            };

            match crate::backdrop::from_bytes(bytes, v("transparent").unwrap_or(0.) != 0.) {
                Ok(image) => {
                    *image_budget = image_budget
                        .checked_sub(image.0.len())
                        .ok_or("صور المستند أكبر من السعة المتاحة")?;
                    page.pictures.push(Picture { image, rect });
                }
                Err(_) => page.missing_pictures += 1,
            }
        }
        page.lines.clear();
        for i in 1..=n as usize {
            let val = |s: &str| get(&format!("{s}0_{i}"));
            let (x, y, xx, yy) = (val("lx")?, val("ly")?, val("lxx")?, val("lyy")?);
            let mut content = Document::default();
            let center = ((x + xx) / 2., (y + yy) / 2.);
            let font_style = val("lfntstyle").ok().map(|v| v as u8);
            content.style.font = match font_style {
                Some(1 | 11) => Font::Nastaliq,
                Some(12) => Font::Kufi,
                _ => Font::Amiri,
            };
            content.style.custom_ink = val("lfntcolor").ok().and_then(ink);
            let reference = KlxReference {
                font_style,
                line_codes: self
                    .text
                    .get(&format!("ltextstr0_{i}"))
                    .cloned()
                    .unwrap_or_default(),
                attachments: self
                    .attachments
                    .get(&format!("attachedto0_{i}"))
                    .cloned()
                    .unwrap_or_default(),
                parts: self.anchors(i, "att", val("ltextcount").unwrap_or(0.), center)?,
                marks: self.anchors(i, "pun", val("lpunccount").unwrap_or(0.), center)?,
            };
            let codes = reference.line_codes.as_slice();
            let text = if klx_text::placeholder(codes) {
                reference
                    .parts
                    .iter()
                    .filter(|p| !klx_text::placeholder(&p.codes))
                    .map(|p| klx_text::decode(&p.codes))
                    .collect::<String>()
            } else {
                klx_text::decode(codes)
            };
            if !document::valid_text(&text) {
                return Err("يتجاوز النص المستورد سعة السطر".into());
            }
            content.edit_text(text);
            let angle = (yy - y).atan2(xx - x).to_degrees();
            content.style.angle = if angle > 90. {
                angle - 180.
            } else if angle < -90. {
                angle + 180.
            } else {
                angle
            };
            if let Ok(size) = val("lfntsize") {
                content.style.size = (size * 4. / 3. / 112.).clamp(0.2, 3.);
            }
            page.lines.push(Layer {
                reference: Some(reference),
                content,
                x: (x + xx) / (2. * w),
                y: (y + yy) / (2. * h),
                baseline: Some([
                    (x - xx) / 30.,
                    (y - yy) / 30.,
                    (xx - x) / 30.,
                    (yy - y) / 30.,
                ]),
            });
        }
        if page.lines.is_empty() {
            let mut blank = Page::default().lines.remove(0);
            blank.content.edit_text(String::new());
            page.lines.push(blank);
        }
        place_words(&mut page);
        Ok(page)
    }
}
// Only place words when their full sequence agrees with the source fragments.
// Fragment positions are right-hand baseline origins in page coordinates. Typography-only
// variants in fragments can differ from the logical line without changing a word.
fn place_words(page: &mut Page) {
    let board = Board::new(page);
    for (layer, arrangement) in page.lines.iter_mut().zip(board.lines) {
        let Some(reference) = &layer.reference else {
            continue;
        };
        let significant = |c: &char| !c.is_whitespace() && !matches!(c, 'ـ' | '�');
        let text = &layer.content.text;
        let mut source = Vec::new();
        let mut anchors = BTreeMap::new();
        for part in &reference.parts {
            let fragment = klx_text::decode(&part.codes);
            let letters: Vec<_> = fragment.chars().filter(significant).collect();
            if !letters.is_empty() {
                anchors.insert(source.len(), part);
                source.extend(letters);
            }
        }
        if source.is_empty() || !source.iter().copied().eq(text.chars().filter(significant)) {
            continue;
        }
        let mut colors = reference.parts.iter().flat_map(|p| {
            klx_text::decode(&p.codes)
                .chars()
                .filter(significant)
                .map(|_| p.ink)
                .collect::<Vec<_>>()
        });
        for (byte, c) in text.char_indices() {
            if significant(&c)
                && let Some(Some(color)) = colors.next()
                && Some(color) != layer.content.style.custom_ink
            {
                layer.content.letter_inks.insert(byte, color);
            }
        }
        struct Placement {
            word: usize,
            anchor: (f64, f64),
            right: f64,
            left: f64,
            baseline: f64,
            top: f64,
            bottom: f64,
        }
        let mut cursor = 0;
        let mut placements = Vec::new();
        for (word, token) in document::words(text) {
            let part = anchors.get(&cursor);
            cursor += token.chars().filter(significant).count();
            let Some(part) = part else { continue };
            let mut glyphs = arrangement.scene.glyphs.iter().filter(|g| g.word == word);
            let Some(first) = glyphs.next() else { continue };
            let baseline = arrangement
                .scene
                .glyphs
                .iter()
                .find(|g| g.word == word && g.mark.is_none())
                .unwrap_or(first)
                .y;
            let bounds = glyphs.fold(first.bounds, |r, g| {
                [
                    r[0].min(g.bounds[0]),
                    r[1].min(g.bounds[1]),
                    r[2].max(g.bounds[2]),
                    r[3].max(g.bounds[3]),
                ]
            });
            placements.push(Placement {
                word,
                anchor: (
                    layer.x * page.width + part.x,
                    layer.y * page.height + part.y,
                ),
                left: bounds[0],
                top: bounds[1],
                right: bounds[2],
                bottom: bounds[3],
                baseline,
            });
        }
        // Independently anchored words can span several visual rows. Fitting the
        // entire logical string to one page width would shrink those rows twice.
        let mut fit = if placements.len() == document::words(text).len() {
            (arrangement.scene.width / page.width).max(1.)
        } else {
            1.
        };
        let frame = arrangement.frame;
        let (sin, cos) = frame.angle.to_radians().sin_cos();
        for (index, p) in placements.iter().enumerate() {
            let (x, y) = p.anchor;
            if !(0.0..=page.width).contains(&x) || !(0.0..=page.height).contains(&y) {
                continue;
            }
            for dx in [p.left - p.right, 0.] {
                for dy in [p.top - p.baseline, p.bottom - p.baseline] {
                    let vx = frame.scale * (cos * dx - sin * dy);
                    let vy = frame.scale * (sin * dx + cos * dy);
                    for (coordinate, delta, limit) in [(x, vx, page.width), (y, vy, page.height)] {
                        if delta < 0. {
                            fit = fit.min((coordinate - 2.) / -delta);
                        }
                        if delta > 0. {
                            fit = fit.min((limit - 2. - coordinate) / delta);
                        }
                    }
                }
            }
            if let Some(next) = placements.get(index + 1) {
                let (advance, rise) = frame.inverse_delta(x - next.anchor.0, y - next.anchor.1);
                let rows_overlap = p.top - p.baseline < next.bottom - next.baseline - rise
                    && p.bottom - p.baseline > next.top - next.baseline - rise;
                if rows_overlap && advance > 0. && p.right > p.left {
                    fit = fit.min(0.94 * advance / (p.right - p.left));
                }
            }
        }
        layer.content.style.size = (layer.content.style.size * fit).clamp(0.2, 3.);
        let frame =
            crate::board::Frame::for_line(layer, &arrangement.scene, page.width, page.height);
        for p in placements {
            let word = p.word;
            let (x, y) = frame.point(p.right, p.baseline);
            let (x, y) = frame.inverse_delta(p.anchor.0 - x, p.anchor.1 - y);
            let offset = Offset { x, y };
            if offset.within(document::MAX_WORD_OFFSET) {
                layer.content.words.insert(word, offset);
            }
        }
    }
}
pub fn load(path: &Path) -> Result<Book> {
    if fs::metadata(path).map_err(|e| e.to_string())?.len() > 32_000_000 {
        return Err("ملف KLX أكبر من الحد المتاح".into());
    }
    let b = fs::read(path).map_err(|e| e.to_string())?;
    if u16at(&b, 0)? != 0x2011 {
        return Err("الملف ليس مستند KLX".into());
    }
    let root = bag(&b[2..], 0)?;
    let mut image_budget = 24_000_000;
    let pages = if root.pages.is_empty() {
        vec![root.page(&mut image_budget)?]
    } else {
        root.pages
            .values()
            .map(|p| p.page(&mut image_budget))
            .collect::<Result<Vec<_>>>()?
    };
    let book = Book { version: 2, pages };
    book.valid()
        .then_some(book)
        .ok_or("إحداثيات KLX خارج النطاق".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(root: &Path, filename: &str) -> std::path::PathBuf {
        fn find(dir: &Path, name: &str) -> Option<std::path::PathBuf> {
            for entry in fs::read_dir(dir).ok()?.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(found) = find(&path, name) {
                        return Some(found);
                    }
                } else if path
                    .file_name()
                    .is_some_and(|f| f.eq_ignore_ascii_case(name))
                {
                    return Some(path);
                }
            }
            None
        }
        find(root, filename).unwrap_or_else(|| panic!("missing test fixture: {filename}"))
    }
    #[test]
    fn invalid_container_is_rejected() {
        for bytes in [
            vec![],
            vec![0; 30],
            vec![1, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        ] {
            assert!(bag(&bytes, 0).is_err());
        }
    }
    #[test]
    #[ignore = "run against the unpacked local KLX Samples when available"]
    fn shipped_layouts() {
        #[derive(Default, Debug)]
        struct Totals {
            pictures: usize,
            missing_pictures: usize,
            visible_guides: usize,
            other_guides: usize,
            multicolor_lines: usize,
            files: usize,
            lines: usize,
            texts: usize,
            partial: usize,
            unknown: usize,
            words: usize,
            parts: usize,
            marks: usize,
        }
        fn visit(root: &Path, totals: &mut Totals) {
            for entry in fs::read_dir(root).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    visit(&path, totals);
                } else if path
                    .extension()
                    .is_some_and(|s| s.eq_ignore_ascii_case("klx"))
                {
                    let book = load(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                    totals.files += 1;
                    for p in &book.pages {
                        totals.pictures += p.pictures.len();
                        totals.missing_pictures += p.missing_pictures;
                        for g in &p.source_guides {
                            if g.svg_element().is_some() {
                                totals.visible_guides += 1;
                            } else {
                                totals.other_guides += 1;
                            }
                        }
                        totals.multicolor_lines += p
                            .lines
                            .iter()
                            .filter(|l| !l.content.letter_inks.is_empty())
                            .count();
                    }
                    for l in book.pages.iter().flat_map(|p| &p.lines) {
                        totals.lines += 1;
                        totals.texts += usize::from(!l.content.text.trim().is_empty());
                        totals.partial += usize::from(l.content.text.contains('�'));
                        totals.unknown += l.content.text.matches('�').count();
                        totals.words += l.content.words.len();
                        if let Some(r) = &l.reference {
                            totals.parts += r.parts.len();
                            totals.marks += r.marks.len();
                        }
                        assert!(!matches!(l.content.text.trim(), "$*&!" | "??=@"));
                    }
                }
            }
        }
        let root = std::env::var_os("MIDAD_KLX_SAMPLES").expect("set MIDAD_KLX_SAMPLES");
        let mut totals = Totals::default();
        visit(Path::new(&root), &mut totals);
        println!("{totals:?}");
        assert!(totals.files > 0 && totals.texts > 0);
        assert_eq!(
            totals.other_guides, 0,
            "all guides in the shipped corpus have visible geometry"
        );
    }
    #[test]
    fn source_guide_bounds_and_visibility() {
        let guide = |kind, points| SourceGuide { kind, points };
        assert_eq!(
            guide(3, [25., 15., 5., -5.]).svg_element().as_deref(),
            Some(r#"<circle cx="15" cy="5" r="10"/>"#)
        );
        assert_eq!(
            guide(4, [60., -20., -20., 20.]).svg_element().as_deref(),
            Some(r#"<ellipse cx="20" cy="0" rx="40" ry="20"/>"#)
        );
        assert!(guide(4, [3., 4., 3., 20.]).svg_element().is_none());
        assert!(guide(99, [0., 0., 1., 1.]).svg_element().is_none());
    }
    #[test]
    #[ignore = "requires the original KLX sample boards"]
    fn original_curved_guides_toggle_and_reopen() {
        let root = std::env::var_os("MIDAD_KLX_SAMPLES").expect("set MIDAD_KLX_SAMPLES");
        let dir = tempfile::tempdir().unwrap();
        for (name, circles, ellipses) in [("Klx04Sls3", 3, 0), ("Klx04Sls7", 0, 1)] {
            let source = fixture(Path::new(&root), &format!("{name}.klx"));
            let book = load(&source).unwrap();
            let page = &book.pages[0];
            let board = Board::new(page);
            let shown = board.svg(page, None, Some(0));
            let group = shown
                .split_once(r#"id="klx-guides""#)
                .unwrap()
                .1
                .split_once("</g>")
                .unwrap()
                .0;
            assert_eq!(group.matches("<circle ").count(), circles);
            assert_eq!(group.matches("<ellipse ").count(), ellipses);
            assert!(!board.svg(page, None, None).contains("klx-guides"));
            let exported = crate::output::encode(&book, 0, "svg").unwrap();
            assert!(!String::from_utf8(exported).unwrap().contains("klx-guides"));
            let mut no_curves = page.clone();
            no_curves.source_guides.retain(|g| g.kind <= 2);
            let raster = |svg: &str| {
                let tree = resvg::usvg::Tree::from_str(svg, &Default::default()).unwrap();
                let mut pixels = resvg::tiny_skia::Pixmap::new(
                    page.width.ceil() as u32,
                    page.height.ceil() as u32,
                )
                .unwrap();
                resvg::render(
                    &tree,
                    resvg::tiny_skia::Transform::identity(),
                    &mut pixels.as_mut(),
                );
                pixels
            };
            let before = raster(&board.svg(&no_curves, None, Some(0)));
            let after = raster(&shown);
            assert!(
                before
                    .pixels()
                    .iter()
                    .zip(after.pixels())
                    .filter(|(a, b)| a != b)
                    .count()
                    > 500
            );
            let path = dir.path().join(format!("{name}.midad"));
            book.save(&path).unwrap();
            let reopened = Book::load(&path).unwrap();
            assert_eq!(reopened.pages[0].source_guides, page.source_guides);
            assert_eq!(
                Board::new(&reopened.pages[0]).svg(&reopened.pages[0], None, Some(0)),
                shown
            );
        }
    }
    #[test]
    fn imported_and_edited_inks_follow_text() {
        assert_eq!(ink(255.), Some([255, 0, 0]));
        assert_eq!(ink(16711680.), Some([0, 0, 255]));
        assert_eq!(ink(-1.), None);
        let mut doc = Document::default();
        doc.edit_text("حبر مداد".into());
        doc.set_word_ink(1, Some([12, 34, 56]));
        doc.edit_text("لون حبر مداد".into());
        let start = doc.text.find("مداد").unwrap();
        assert_eq!(
            doc.letter_inks.keys().copied().collect::<Vec<_>>(),
            (start..doc.text.len()).step_by(2).collect::<Vec<_>>()
        );
        doc.set_word_ink(2, None);
        assert!(doc.letter_inks.is_empty() && doc.valid());
    }
    #[test]
    #[ignore = "verify original artwork, reflected placements, guides and color in the installed KLX samples"]
    fn decorated_import_roundtrip() {
        let root = std::env::var_os("MIDAD_KLX_SAMPLES").unwrap();
        let book = load(&fixture(Path::new(&root), "Klx05Nsk1.klx")).unwrap();
        let page = &book.pages[0];
        assert_eq!((page.pictures.len(), page.missing_pictures), (7, 0));
        assert_eq!(page.pictures[1].rect, [2., 797., 154., -154.]);
        assert!(page.source_guides.iter().any(|g| g.kind == 2));
        let tmp = tempfile::tempdir().unwrap();
        let dir = std::env::var_os("MIDAD_QA_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| tmp.path().into());
        let path = dir.join("ornaments.midad");
        book.save(&path).unwrap();
        assert_eq!(book, Book::load(&path).unwrap());
        for ext in ["png", "svg", "pdf"] {
            crate::output::export(&book, 0, ext, &dir.join(format!("ornaments.{ext}"))).unwrap();
        }
        let svg = fs::read_to_string(dir.join("ornaments.svg")).unwrap();
        assert_eq!(svg.matches("<image ").count(), 7);
        assert!(!svg.contains("stroke-dasharray"));
        let mut hidden = book.clone();
        hidden.pages[0].hide_pictures = true;
        let bare = crate::output::encode(&hidden, 0, "png").unwrap();
        let bare = image::load_from_memory(&bare).unwrap().into_rgb8();
        let decorated = image::open(dir.join("ornaments.png")).unwrap().into_rgb8();
        let difference = decorated
            .pixels()
            .zip(bare.pixels())
            .filter(|(a, b)| a != b)
            .count();
        assert!(difference > 1500);
    }
    #[test]
    #[ignore = "check the locally installed KLX sample, including the native save and export paths"]
    fn basmala_import_roundtrip() {
        let root = std::env::var_os("MIDAD_KLX_SAMPLES").expect("set MIDAD_KLX_SAMPLES");
        let sample = fixture(Path::new(&root), "Klx07Kha2.klx");
        let book = load(&sample).unwrap();
        let page = &book.pages[0];
        let line = &page.lines[0];
        assert_eq!(line.content.text, "بسم الله الرحمن الرحیم");
        assert_eq!(line.content.words.len(), 4);
        let reference = line.reference.as_ref().unwrap();
        assert_eq!((reference.parts.len(), reference.marks.len()), (6, 12));
        assert_eq!(reference.parts[0].shape, Some(2));
        assert_eq!(reference.attachments.len(), 12);
        let guides = Board::new(page).svg(page, None, Some(0));
        assert!(guides.contains("<circle"));
        let dir = tempfile::tempdir().unwrap();
        let out = std::env::var_os("MIDAD_QA_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| dir.path().into());
        let path = out.join("basmala.midad");
        book.save(&path).unwrap();
        assert_eq!(Book::load(&path).unwrap(), book);
        for ext in ["svg", "pdf", "png"] {
            crate::output::export(&book, 0, ext, &out.join(format!("basmala.{ext}"))).unwrap();
        }
        assert!(
            !fs::read_to_string(out.join("basmala.svg"))
                .unwrap()
                .contains("<circle")
        );
        let png = image::open(out.join("basmala.png")).unwrap().into_rgb8();
        assert!(png.pixels().filter(|p| p[0] < 100).count() > 1000);
        // No imported word should be clipped by the edge of this known composition.
        assert!(png.enumerate_pixels().all(|(x, y, p)| p[0] >= 100
            || (x > 0 && y > 0 && x + 1 < png.width() && y + 1 < png.height())));
    }
}

use crate::{board::Board, book::Book, document::atomic_write};
use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref};
use resvg::{tiny_skia, usvg};
use std::{collections::HashMap, path::Path};
pub fn export(book: &Book, page: usize, ext: &str, path: &Path) -> Result<(), String> {
    atomic_write(path, &encode(book, page, ext)?).map_err(|e| e.to_string())
}
pub fn encode(book: &Book, page: usize, ext: &str) -> Result<Vec<u8>, String> {
    let build = |n: usize| {
        let p = &book.pages[n];
        Board::new(p).svg(p, None, None)
    };
    let tree = |n| usvg::Tree::from_str(&build(n), &Default::default()).map_err(|e| e.to_string());
    let bytes = match ext {
        "svg" => build(page).into_bytes(),
        "png" => {
            let t = tree(page)?;
            let size = t.size();
            let factor = (3200. / size.width().max(size.height())).min(2.);
            let mut bitmap = tiny_skia::Pixmap::new(
                (size.width() * factor).round().max(1.) as u32,
                (size.height() * factor).round().max(1.) as u32,
            )
            .ok_or("تعذّر إنشاء الصورة")?;
            resvg::render(
                &t,
                tiny_skia::Transform::from_scale(factor, factor),
                &mut bitmap.as_mut(),
            );
            bitmap.encode_png().map_err(|e| e.to_string())?
        }
        "pdf" => {
            let mut pdf = Pdf::new();
            let mut ids = Ref::new(1);
            let catalog = ids.bump();
            let root = ids.bump();
            let mut pages = vec![];
            pdf.catalog(catalog).pages(root);
            for i in 0..book.pages.len() {
                let t = tree(i)?;
                let (chunk, obj) =
                    svg2pdf::to_chunk(&t, Default::default()).map_err(|e| e.to_string())?;
                let mut map = HashMap::new();
                let chunk = chunk.renumber(|k| *map.entry(k).or_insert_with(|| ids.bump()));
                let page = ids.bump();
                let stream = ids.bump();
                pages.push(page);
                let (w, h) = (t.size().width() * 0.75, t.size().height() * 0.75);
                let mut p = pdf.page(page);
                p.parent(root)
                    .media_box(Rect::new(0., 0., w, h))
                    .contents(stream);
                p.resources().x_objects().pair(Name(b"Art"), map[&obj]);
                p.finish();
                let mut content = Content::new();
                content
                    .transform([w, 0., 0., h, 0., 0.])
                    .x_object(Name(b"Art"));
                pdf.stream(stream, &content.finish());
                pdf.extend(&chunk);
            }
            pdf.pages(root).count(pages.len() as i32).kids(pages);
            pdf.finish()
        }
        _ => return Err("صيغة تصدير غير مدعومة".into()),
    };
    Ok(bytes)
}

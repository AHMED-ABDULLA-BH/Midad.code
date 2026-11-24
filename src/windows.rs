// Native Unicode controls provide bidirectional editing. The canvas uses the same
// shaped outlines and page coordinates as the other desktop frontend.
#![allow(unsafe_op_in_unsafe_fn)]
use crate::{
    board::{Board, Target},
    book::{Book, Font, INKS, MAX_LINES, MAX_PAGES, Page},
    document::{self, MAX_STRETCH},
    klx, output, profile,
};
use std::{
    cell::RefCell,
    ffi::OsString,
    mem::{size_of, zeroed},
    os::windows::ffi::OsStringExt,
    path::{Path, PathBuf},
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::{LibraryLoader::GetModuleHandleW, SystemServices::SS_RIGHT},
    UI::{
        Controls::Dialogs::*,
        Input::KeyboardAndMouse::{ReleaseCapture, SetCapture},
        WindowsAndMessaging::*,
    },
};

const TEXT: i32 = 101;
const PAGE: i32 = 102;
const LINE: i32 = 103;
const FONT: i32 = 104;
const INK: i32 = 105;
const SIZE: i32 = 106;
const MODE: i32 = 107;
const JOINT: i32 = 108;
const GUIDES: i32 = 109;
const STATUS: i32 = 110;
const ADD_PAGE: i32 = 111;
const ADD_LINE: i32 = 112;
const LONGER: i32 = 113;
const SHORTER: i32 = 114;
const SPLIT: i32 = 115;
const NEW: i32 = 201;
const OPEN: i32 = 202;
const SAVE: i32 = 203;
const SAVE_AS: i32 = 204;
const SVG: i32 = 205;
const PNG: i32 = 206;
const PDF: i32 = 207;
const UNDO: i32 = 208;
const REDO: i32 = 209;
const IMPORT: i32 = 210;
const BACKGROUND: i32 = 211;
const BORDER: i32 = 212;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
unsafe fn title(hwnd: HWND, s: &str) {
    SetWindowTextW(hwnd, wide(s).as_ptr());
}
unsafe fn control(hwnd: HWND, id: i32) -> HWND {
    GetDlgItem(hwnd, id)
}
unsafe fn index(hwnd: HWND, id: i32) -> usize {
    SendMessageW(control(hwnd, id), CB_GETCURSEL, 0, 0).max(0) as usize
}
unsafe fn combo(hwnd: HWND, id: i32, options: impl IntoIterator<Item = String>, selected: usize) {
    let h = control(hwnd, id);
    SendMessageW(h, CB_RESETCONTENT, 0, 0);
    for text in options {
        SendMessageW(h, CB_ADDSTRING, 0, wide(&text).as_ptr() as isize);
    }
    SendMessageW(h, CB_SETCURSEL, selected, 0);
}
unsafe fn control_text(hwnd: HWND) -> String {
    let mut buf = vec![0u16; GetWindowTextLengthW(hwnd) as usize + 1];
    let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
    String::from_utf16_lossy(&buf[..n as usize])
}
unsafe fn choose(
    hwnd: HWND,
    save: bool,
    description: &str,
    extensions: &str,
    default: &str,
) -> Option<PathBuf> {
    let filter = wide(&format!("{description}\0{extensions}\0"));
    let ext = wide(default);
    let mut path = vec![0u16; 32768];
    let mut dialog: OPENFILENAMEW = zeroed();
    dialog.lStructSize = size_of::<OPENFILENAMEW>() as u32;
    dialog.hwndOwner = hwnd;
    dialog.lpstrFilter = filter.as_ptr();
    dialog.lpstrFile = path.as_mut_ptr();
    dialog.nMaxFile = path.len() as u32;
    dialog.lpstrDefExt = ext.as_ptr();
    dialog.Flags = OFN_PATHMUSTEXIST
        | OFN_NOCHANGEDIR
        | OFN_EXPLORER
        | if save {
            OFN_OVERWRITEPROMPT
        } else {
            OFN_FILEMUSTEXIST
        };
    let ok = if save {
        GetSaveFileNameW(&mut dialog)
    } else {
        GetOpenFileNameW(&mut dialog)
    };
    (ok != 0).then(|| {
        PathBuf::from(OsString::from_wide(
            &path[..path.iter().position(|&c| c == 0).unwrap_or(path.len())],
        ))
    })
}
fn err(hwnd: HWND, s: &str) {
    unsafe {
        MessageBoxW(
            hwnd,
            wide(s).as_ptr(),
            wide("مِداد").as_ptr(),
            MB_OK | MB_ICONERROR | MB_RTLREADING,
        );
    }
}
#[derive(Clone)]
struct Work {
    book: Book,
    page: usize,
    line: usize,
}
impl Default for Work {
    fn default() -> Self {
        Self {
            book: Book::default(),
            page: 0,
            line: 0,
        }
    }
}
impl Work {
    fn layer(&mut self) -> &mut crate::book::Layer {
        &mut self.book.pages[self.page].lines[self.line]
    }
    fn doc(&mut self) -> &mut document::Document {
        &mut self.layer().content
    }
}
#[derive(Default)]
struct Studio {
    work: Work,
    saved: Book,
    path: Option<PathBuf>,
    undo: Vec<Work>,
    redo: Vec<Work>,
    guides: bool,
    selection: Option<Target>,
    drag: Option<(f64, f64)>,
    drag_saved: bool,
    cache: Option<resvg::tiny_skia::Pixmap>,
    frame: (i32, i32, i32, i32),
    board: Option<Board>,
}
thread_local! {static STUDIO:RefCell<Studio>=RefCell::new(Studio::default());}
impl Studio {
    fn checkpoint(&mut self) {
        if self.undo.len() >= 40 {
            self.undo.remove(0);
        }
        self.undo.push(self.work.clone());
        self.redo.clear();
    }
    unsafe fn redraw(&mut self, h: HWND) {
        self.cache = None;
        self.board = None;
        InvalidateRect(h, null(), 0);
    }
    unsafe fn sync(&mut self, h: HWND) {
        let work = &self.work;
        let p = &work.book.pages[work.page];
        let d = &p.lines[work.line].content;
        let field = control(h, TEXT);
        if control_text(field) != d.text {
            title(field, &d.text);
        }
        combo(
            h,
            PAGE,
            (1..=work.book.pages.len()).map(|n| format!("صفحة {n}")),
            work.page,
        );
        combo(
            h,
            LINE,
            p.lines.iter().enumerate().map(|(i, l)| {
                format!(
                    "{} · {}",
                    i + 1,
                    l.content.text.chars().take(19).collect::<String>()
                )
            }),
            work.line,
        );
        let fonts = Font::available();
        combo(
            h,
            FONT,
            fonts.iter().map(|f| f.name().into()),
            fonts.iter().position(|&f| f == d.style.font).unwrap_or(0),
        );
        let special = d.style.custom_ink.map(crate::book::hex_color);
        combo(
            h,
            INK,
            INKS.iter().map(|i| i.0.to_owned()).chain(special.clone()),
            if special.is_some() {
                INKS.len()
            } else {
                d.style.ink
            },
        );
        let sizes = [0.25, 0.5, 0.75, 1., 1.25, 1.5, 2., 2.5, 3.];
        let mut values = sizes.to_vec();
        let at = values
            .iter()
            .position(|&v| v == d.style.size)
            .unwrap_or_else(|| {
                values.push(d.style.size);
                values.len() - 1
            });
        combo(
            h,
            SIZE,
            values.iter().map(|v| format!("{:.0}٪", v * 100.)),
            at,
        );
        let joins = document::joins(&d.text);
        let j = index(h, JOINT).min(joins.len().saturating_sub(1));
        combo(
            h,
            JOINT,
            joins
                .iter()
                .enumerate()
                .map(|(i, (_, s))| format!("{} · {s}", i + 1)),
            j,
        );
        title(
            control(h, GUIDES),
            if self.guides {
                "إخفاء الأدلة"
            } else {
                "إظهار الأدلة"
            },
        );
        let name = self
            .path
            .as_ref()
            .and_then(|p| p.file_stem())
            .and_then(|n| n.to_str())
            .unwrap_or("لوحة");
        title(
            h,
            &format!(
                "{name}{} — مِداد",
                if self.saved == work.book { "" } else { " *" }
            ),
        );
        self.redraw(h);
    }
    unsafe fn save(&mut self, h: HWND, as_new: bool) -> bool {
        let path = if as_new { None } else { self.path.clone() }
            .or_else(|| choose(h, true, "لوحة مداد", "*.midad", "midad"));
        let Some(path) = path else { return false };
        if let Err(e) = self.work.book.save(&path) {
            err(h, &e);
            return false;
        }
        self.path = Some(path);
        self.saved = self.work.book.clone();
        self.sync(h);
        true
    }
    unsafe fn may_leave(&mut self, h: HWND) -> bool {
        if self.work.book == self.saved {
            return true;
        }
        match MessageBoxW(
            h,
            wide("هل تريد حفظ التعديلات؟").as_ptr(),
            wide("مِداد").as_ptr(),
            MB_YESNOCANCEL | MB_ICONQUESTION | MB_RTLREADING,
        ) {
            IDYES => self.save(h, false),
            IDNO => true,
            _ => false,
        }
    }
    unsafe fn open(&mut self, h: HWND, path: &Path) {
        let import = path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("klx"));
        match if import {
            klx::load(path)
        } else {
            Book::load(path)
        } {
            Ok(book) => {
                self.work = Work {
                    book,
                    page: 0,
                    line: 0,
                };
                self.saved = if import {
                    Book::default()
                } else {
                    self.work.book.clone()
                };
                self.guides = import;
                self.path = (!import).then(|| path.to_owned());
                self.undo.clear();
                self.redo.clear();
                self.selection = None;
                self.sync(h);
            }
            Err(e) => err(h, &e),
        }
    }
    unsafe fn paint(&mut self, h: HWND) {
        let mut paint: PAINTSTRUCT = zeroed();
        let dc = BeginPaint(h, &mut paint);
        let mut client: RECT = zeroed();
        GetClientRect(h, &mut client);
        let p = &self.work.book.pages[self.work.page];
        let aw = (client.right - 275).max(1);
        let ah = (client.bottom - 28).max(1);
        let scale = (aw as f64 / p.width).min(ah as f64 / p.height);
        let w = (p.width * scale).round().max(1.) as i32;
        let height = (p.height * scale).round().max(1.) as i32;
        let x = (aw - w) / 2 + 8;
        let y = (ah - height) / 2 + 8;
        FillRect(dc, &paint.rcPaint, GetSysColorBrush(COLOR_BTNFACE));
        self.frame = (x, y, w, height);
        if self.cache.is_none() {
            let board = Board::new(p);
            let svg = board.svg(
                p,
                self.selection.as_ref().map(|t| (self.work.line, t)),
                self.guides.then_some(self.work.line),
            );
            if let Ok(tree) = resvg::usvg::Tree::from_str(&svg, &Default::default()) {
                let ratio = scale.min(3200. / p.width.max(p.height));
                if let Some(mut im) = resvg::tiny_skia::Pixmap::new(
                    (p.width * ratio).round().max(1.) as u32,
                    (p.height * ratio).round().max(1.) as u32,
                ) {
                    resvg::render(
                        &tree,
                        resvg::tiny_skia::Transform::from_scale(ratio as f32, ratio as f32),
                        &mut im.as_mut(),
                    );
                    for px in im.data_mut().chunks_exact_mut(4) {
                        px.swap(0, 2);
                    }
                    self.cache = Some(im);
                }
            }
            self.board = Some(board);
        }
        if let Some(im) = &self.cache {
            let mut info: BITMAPINFO = zeroed();
            info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
            info.bmiHeader.biWidth = im.width() as i32;
            info.bmiHeader.biHeight = -(im.height() as i32);
            info.bmiHeader.biPlanes = 1;
            info.bmiHeader.biBitCount = 32;
            info.bmiHeader.biCompression = BI_RGB;
            StretchDIBits(
                dc,
                x,
                y,
                w,
                height,
                0,
                0,
                im.width() as i32,
                im.height() as i32,
                im.data().as_ptr().cast(),
                &info,
                DIB_RGB_COLORS,
                SRCCOPY,
            );
        }
        EndPaint(h, &paint);
    }
    fn point(&self, lp: LPARAM) -> (f64, f64) {
        let (x, y, w, h) = self.frame;
        let p = &self.work.book.pages[self.work.page];
        (
            (((lp as i16) as i32 - x) as f64 * p.width / w.max(1) as f64),
            ((((lp >> 16) as i16) as i32 - y) as f64 * p.height / h.max(1) as f64),
        )
    }
    unsafe fn mouse(&mut self, h: HWND, msg: u32, lp: LPARAM) {
        let (x, y) = self.point(lp);
        if msg == WM_LBUTTONDOWN {
            if let Some((line, t)) = self
                .board
                .as_ref()
                .and_then(|b| b.hit(x, y, index(h, MODE), 5.))
            {
                self.work.line = line;
                self.selection = Some(t);
                self.drag = Some((x, y));
                self.drag_saved = false;
                SetCapture(h);
                self.sync(h);
            }
        } else if msg == WM_LBUTTONUP {
            self.drag = None;
            ReleaseCapture();
        } else if let (Some((px, py)), Some(t)) = (self.drag, self.selection.clone()) {
            let (dx, dy) = (x - px, y - py);
            if dx == 0. && dy == 0. {
                return;
            }
            if !self.drag_saved {
                self.checkpoint();
                self.drag_saved = true;
            }
            let p = &mut self.work.book.pages[self.work.page];
            let l = &mut p.lines[self.work.line];
            if t == Target::Line {
                l.x = (l.x + dx / p.width).clamp(-64., 64.);
                l.y = (l.y + dy / p.height).clamp(-64., 64.);
            } else {
                let scene = crate::scene::Scene::shape(&l.content);
                let frame = crate::board::Frame::for_line(l, &scene, p.width, p.height);
                let (dx, dy) = frame.inverse_delta(dx, dy);
                let d = &mut l.content;
                let (o, limit) = match t {
                    Target::Word(k) => (d.words.entry(k).or_default(), document::MAX_WORD_OFFSET),
                    Target::Letter(k) => (d.letters.entry(k).or_default(), 500.),
                    Target::Mark(k) => (d.marks.entry(k).or_default(), 60.),
                    Target::Part(k) => (d.parts.entry(k).or_default(), 100.),
                    _ => return,
                };
                o.x = (o.x + dx).clamp(-limit, limit);
                o.y = (o.y + dy).clamp(-limit, limit);
            }
            self.drag = Some((x, y));
            self.redraw(h);
        }
    }
    unsafe fn command(&mut self, h: HWND, id: i32, notification: u32) {
        if id == TEXT {
            if notification != EN_CHANGE {
                return;
            }
            let s = control_text(control(h, TEXT));
            if s == self.work.doc().text {
                return;
            }
            if !document::valid_text(&s) {
                self.sync(h);
                return;
            }
            self.checkpoint();
            self.work.doc().edit_text(s);
        } else if matches!(id, PAGE | LINE | FONT | SIZE | INK | MODE | JOINT) {
            if notification != CBN_SELCHANGE {
                return;
            }
            let i = index(h, id);
            match id {
                PAGE => {
                    self.work.page = i;
                    self.work.line = 0;
                    self.selection = None;
                }
                LINE => {
                    self.work.line = i;
                    self.selection = None;
                }
                FONT => {
                    self.checkpoint();
                    profile::change_font(self.work.doc(), Font::available()[i]);
                }
                SIZE => {
                    if let Some(&n) = [0.25, 0.5, 0.75, 1., 1.25, 1.5, 2., 2.5, 3.].get(i) {
                        self.checkpoint();
                        self.work.doc().style.size = n;
                    }
                }
                INK => {
                    if i < INKS.len() {
                        self.checkpoint();
                        let s = &mut self.work.doc().style;
                        s.ink = i;
                        s.custom_ink = None;
                    }
                }
                MODE => self.selection = None,
                _ => {}
            }
        } else {
            match id {
                NEW => {
                    if self.may_leave(h) {
                        *self = Self::default();
                    }
                }
                OPEN | IMPORT => {
                    let dialog = if id == OPEN {
                        ("لوحة مداد", "*.midad", "midad")
                    } else {
                        ("KLX", "*.klx", "klx")
                    };
                    if let Some(path) = choose(h, false, dialog.0, dialog.1, dialog.2) {
                        if self.may_leave(h) {
                            self.open(h, &path)
                        }
                    }
                }
                SAVE | SAVE_AS => {
                    self.save(h, id == SAVE_AS);
                }
                PNG | PDF | SVG => {
                    let (ext, filter) = match id {
                        PNG => ("png", "*.png"),
                        PDF => ("pdf", "*.pdf"),
                        _ => ("svg", "*.svg"),
                    };
                    if let Some(path) = choose(h, true, ext, filter, ext) {
                        match output::export(&self.work.book, self.work.page, ext, &path) {
                            Ok(()) => title(control(h, STATUS), "تم تصدير اللوحة"),
                            Err(e) => err(h, &e),
                        }
                    }
                }
                ADD_PAGE if self.work.book.pages.len() < MAX_PAGES => {
                    self.checkpoint();
                    self.work.book.pages.push(Page::default());
                    self.work.page = self.work.book.pages.len() - 1;
                    self.work.line = 0;
                    self.selection = None;
                }
                ADD_LINE if self.work.book.pages[self.work.page].lines.len() < MAX_LINES => {
                    self.checkpoint();
                    let mut layer = self.work.layer().clone();
                    layer.content.edit_text(String::new());
                    layer.y = (layer.y + 0.16).min(0.9);
                    layer.baseline = None;
                    layer.reference = None;
                    let p = &mut self.work.book.pages[self.work.page];
                    p.lines.push(layer);
                    self.work.line = p.lines.len() - 1;
                    self.selection = None;
                }
                LONGER | SHORTER | SPLIT => {
                    let i = index(h, JOINT);
                    if let Some((k, _)) = document::joins(&self.work.doc().text).get(i) {
                        self.checkpoint();
                        let d = self.work.doc();
                        if id == SPLIT {
                            d.stretches.remove(k);
                            if !d.breaks.remove(k) {
                                d.breaks.insert(*k);
                            }
                        } else {
                            d.breaks.remove(k);
                            let n = d.stretches.entry(*k).or_default();
                            *n = if id == LONGER {
                                n.saturating_add(1).min(MAX_STRETCH)
                            } else {
                                n.saturating_sub(1)
                            };
                        }
                    }
                }
                BACKGROUND => {
                    if let Some(path) = choose(h, false, "صورة", "*.png;*.jpg;*.jpeg", "png") {
                        match crate::backdrop::load(&path) {
                            Ok(bg) => {
                                self.checkpoint();
                                self.work.book.pages[self.work.page].background = Some(bg);
                            }
                            Err(e) => err(h, &e),
                        }
                    }
                }
                BORDER => {
                    self.checkpoint();
                    let p = &mut self.work.book.pages[self.work.page];
                    p.border = !p.border;
                }
                GUIDES => self.guides = !self.guides,
                UNDO => {
                    if let Some(prev) = self.undo.pop() {
                        self.redo.push(std::mem::replace(&mut self.work, prev));
                        self.selection = None;
                    }
                }
                REDO => {
                    if let Some(next) = self.redo.pop() {
                        self.undo.push(std::mem::replace(&mut self.work, next));
                        self.selection = None;
                    }
                }
                _ => return,
            }
        }
        self.sync(h);
    }
}

unsafe fn menus(h: HWND) {
    let bar = CreateMenu();
    for (name, entries) in [
        (
            "ملف",
            vec![
                (NEW, "جديد"),
                (OPEN, "فتح لوحة…"),
                (IMPORT, "استيراد KLX…"),
                (SAVE, "حفظ"),
                (SAVE_AS, "حفظ باسم…"),
                (SVG, "تصدير SVG…"),
                (PNG, "تصدير PNG…"),
                (PDF, "تصدير PDF…"),
            ],
        ),
        ("تحرير", vec![(UNDO, "تراجع"), (REDO, "إعادة")]),
        (
            "الصفحة",
            vec![
                (ADD_PAGE, "إضافة صفحة"),
                (ADD_LINE, "إضافة سطر"),
                (BACKGROUND, "صورة خلفية…"),
                (BORDER, "إطار الصفحة"),
                (GUIDES, "إظهار أو إخفاء الأدلة"),
            ],
        ),
    ] {
        let menu = CreatePopupMenu();
        for (id, label) in entries {
            AppendMenuW(menu, MF_STRING, id as usize, wide(label).as_ptr());
        }
        AppendMenuW(bar, MF_POPUP, menu as usize, wide(name).as_ptr());
    }
    SetMenu(h, bar);
}
const CONTROLS: &[(i32, &str, &str, i32, i32, i32, u32)] = &[
    (301, "STATIC", "صفحة اللوحة", 12, 12, 220, SS_RIGHT as u32),
    (PAGE, "COMBOBOX", "", 12, 32, 160, CBS_DROPDOWNLIST as u32),
    (ADD_PAGE, "BUTTON", "＋", 179, 32, 52, 0),
    (302, "STATIC", "السطر", 12, 66, 220, SS_RIGHT as u32),
    (LINE, "COMBOBOX", "", 12, 87, 160, CBS_DROPDOWNLIST as u32),
    (ADD_LINE, "BUTTON", "＋", 179, 87, 52, 0),
    (303, "STATIC", "نص السطر", 12, 124, 220, SS_RIGHT as u32),
    (
        TEXT,
        "EDIT",
        "",
        12,
        146,
        219,
        (ES_RIGHT | ES_AUTOHSCROLL) as u32 | WS_BORDER,
    ),
    (304, "STATIC", "الخط", 12, 185, 220, SS_RIGHT as u32),
    (FONT, "COMBOBOX", "", 12, 205, 219, CBS_DROPDOWNLIST as u32),
    (305, "STATIC", "الحبر والحجم", 12, 242, 220, SS_RIGHT as u32),
    (SIZE, "COMBOBOX", "", 12, 263, 86, CBS_DROPDOWNLIST as u32),
    (INK, "COMBOBOX", "", 104, 263, 127, CBS_DROPDOWNLIST as u32),
    (
        306,
        "STATIC",
        "التحريك على اللوحة",
        12,
        303,
        220,
        SS_RIGHT as u32,
    ),
    (MODE, "COMBOBOX", "", 12, 324, 219, CBS_DROPDOWNLIST as u32),
    (307, "STATIC", "وصلة الحروف", 12, 365, 220, SS_RIGHT as u32),
    (JOINT, "COMBOBOX", "", 12, 386, 219, CBS_DROPDOWNLIST as u32),
    (SHORTER, "BUTTON", "تقصير", 12, 423, 69, 0),
    (LONGER, "BUTTON", "مدّ", 87, 423, 69, 0),
    (SPLIT, "BUTTON", "فصل / وصل", 162, 423, 69, 0),
    (GUIDES, "BUTTON", "إظهار الأدلة", 12, 470, 219, 0),
    (STATUS, "STATIC", "", 12, 519, 219, SS_RIGHT as u32),
];
unsafe fn layout(h: HWND, create: bool) {
    let mut r: RECT = zeroed();
    GetClientRect(h, &mut r);
    let left = r.right - 247;
    for &(id, class, label, x, y, w, style) in CONTROLS {
        let height = if class == "COMBOBOX" {
            240
        } else if class == "STATIC" {
            20
        } else {
            29
        };
        if create {
            let child = CreateWindowExW(
                WS_EX_RTLREADING,
                wide(class).as_ptr(),
                wide(label).as_ptr(),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | style,
                left + x,
                y,
                w,
                height,
                h,
                id as usize as HMENU,
                GetModuleHandleW(null()),
                null(),
            );
            SendMessageW(
                child,
                WM_SETFONT,
                GetStockObject(DEFAULT_GUI_FONT) as usize,
                1,
            );
            if id == TEXT {
                SendMessageW(child, 0x00c5, document::MAX_CHARS * 2, 0);
            }
        } else {
            MoveWindow(control(h, id), left + x, y, w, height, 1);
        }
    }
}
unsafe extern "system" fn procedure(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_DESTROY {
        PostQuitMessage(0);
        return 0;
    }
    if msg == WM_GETMINMAXINFO {
        let limits = &mut *(lp as *mut MINMAXINFO);
        limits.ptMinTrackSize = POINT { x: 720, y: 630 };
        return 0;
    }
    STUDIO.with(|s| {
        let Ok(mut studio) = s.try_borrow_mut() else {
            return DefWindowProcW(h, msg, wp, lp);
        };
        match msg {
            WM_CREATE => {
                menus(h);
                layout(h, true);
                combo(
                    h,
                    MODE,
                    [
                        "تحريك السطر",
                        "تحريك الكلمة",
                        "تحريك الحركة",
                        "تحريك جزء الحرف",
                        "تحريك الحرف",
                    ]
                    .iter()
                    .map(|s| s.to_string()),
                    0,
                );
                studio.sync(h);
            }
            WM_SIZE => {
                layout(h, false);
                studio.redraw(h);
            }
            WM_PAINT => studio.paint(h),
            WM_COMMAND => studio.command(h, (wp & 0xffff) as i32, (wp >> 16) as u32),
            WM_LBUTTONDOWN | WM_MOUSEMOVE | WM_LBUTTONUP => studio.mouse(h, msg, lp),
            WM_CAPTURECHANGED => studio.drag = None,
            WM_CLOSE => {
                if studio.may_leave(h) {
                    DestroyWindow(h);
                }
            }
            _ => return DefWindowProcW(h, msg, wp, lp),
        }
        0
    })
}
pub fn run() {
    unsafe {
        let class = wide("MidadDesktopWindow");
        let instance = GetModuleHandleW(null());
        let mut wc: WNDCLASSW = zeroed();
        wc.hInstance = instance;
        wc.lpszClassName = class.as_ptr();
        wc.lpfnWndProc = Some(procedure);
        wc.hCursor = LoadCursorW(null_mut(), IDC_ARROW);
        wc.hbrBackground = GetSysColorBrush(COLOR_BTNFACE);
        if RegisterClassW(&wc) == 0 {
            err(null_mut(), "تعذّر تسجيل نافذة مداد");
            return;
        }
        let h = CreateWindowExW(
            0,
            class.as_ptr(),
            wide("مِداد").as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            1120,
            740,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        if h.is_null() {
            err(null_mut(), "تعذّر فتح النافذة");
            return;
        }
        if let Some(path) = std::env::args_os().nth(1) {
            STUDIO.with(|s| s.borrow_mut().open(h, Path::new(&path)));
        }
        ShowWindow(h, SW_SHOW);
        UpdateWindow(h);
        let mut msg: MSG = zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            if IsDialogMessageW(h, &msg) == 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}

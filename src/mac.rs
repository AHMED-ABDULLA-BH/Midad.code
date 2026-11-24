use crate::{
    board::{Board, Target},
    book::{Book, Font, INKS, MAX_LINES, MAX_PAGES, PAPERS, Page},
    document::{self, Document},
    klx, output, profile,
};
use objc2::{
    AnyThread, DefinedClass, MainThreadOnly, Message, define_class, msg_send,
    rc::Retained,
    runtime::{ProtocolObject, Sel},
    sel,
};
use objc2_app_kit::*;
use objc2_foundation::{
    MainThreadMarker, NSArray, NSData, NSNotification, NSObject, NSObjectProtocol, NSPoint, NSRect,
    NSSize, NSString,
};
use std::{
    cell::{Cell, OnceCell, RefCell},
    path::{Path, PathBuf},
};

fn s(text: &str) -> Retained<NSString> {
    NSString::from_str(text)
}
fn rect(x: f64, y: f64, w: f64, h: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
}
fn tint(r: f64, g: f64, b: f64) -> Retained<NSColor> {
    NSColor::colorWithSRGBRed_green_blue_alpha(r, g, b, 1.)
}
const WIDTH: f64 = 1150.;
const HEIGHT: f64 = 660.;
const SIDE: f64 = 320.;
const HEADER: f64 = 66.;
struct Controls {
    text: Retained<NSTextField>,
    status: Retained<NSTextField>,
    page: Retained<NSPopUpButton>,
    line: Retained<NSPopUpButton>,
    fonts: Retained<NSPopUpButton>,
    inks: Retained<NSPopUpButton>,
    word_ink: Retained<NSPopUpButton>,
    size: Retained<NSPopUpButton>,
    angle: Retained<NSPopUpButton>,
    curve: Retained<NSPopUpButton>,
    spacing: Retained<NSPopUpButton>,
    mode: Retained<NSPopUpButton>,
    panel: Retained<NSPopUpButton>,
    joins: Retained<NSPopUpButton>,
    word: Retained<NSPopUpButton>,
    forms: Retained<NSPopUpButton>,
    paper: Retained<NSPopUpButton>,
    format: Retained<NSPopUpButton>,
    frame: Retained<NSButton>,
    guides: Retained<NSButton>,
    minus: Retained<NSButton>,
    plus: Retained<NSButton>,
    split: Retained<NSButton>,
    restore: Retained<NSButton>,
    undo: Retained<NSButton>,
    redo: Retained<NSButton>,
    groups: [Vec<Retained<NSView>>; 3],
}
#[derive(Clone, Default)]
struct Work {
    book: Book,
    page: usize,
    line: usize,
}
impl Work {
    fn page(&self) -> &Page {
        &self.book.pages[self.page]
    }
    fn page_mut(&mut self) -> &mut Page {
        &mut self.book.pages[self.page]
    }
    fn doc(&self) -> &Document {
        &self.page().lines[self.line].content
    }
    fn doc_mut(&mut self) -> &mut Document {
        let n = self.line;
        &mut self.page_mut().lines[n].content
    }
}
#[derive(Default)]
struct EditorState {
    work: RefCell<Work>,
    saved: RefCell<Book>,
    path: RefCell<Option<PathBuf>>,
    undo: RefCell<Vec<Work>>,
    redo: RefCell<Vec<Work>>,
    board: RefCell<Option<Board>>,
    image: RefCell<Option<Retained<NSImage>>>,
    selected: RefCell<Option<Target>>,
    drag: Cell<Option<NSPoint>>,
    drag_started: Cell<bool>,
    show_guides: Cell<bool>,
    zoom_level: Cell<i32>,
    pan: Cell<(f64, f64)>,
    form_choices: RefCell<Vec<(&'static str, &'static str)>>,

    ui: OnceCell<Controls>,
}
const SIZES: &[f64] = &[0.35, 0.5, 0.65, 0.8, 1., 1.2, 1.5, 2., 2.5, 3.];
const ANGLES: &[f64] = &[-90., -45., -30., -15., -5., 0., 5., 15., 30., 45., 90.];
const CURVES: &[f64] = &[-150., -90., -45., 0., 45., 90., 150.];
const SPACES: &[f64] = &[-30., -15., 0., 15., 30., 55., 80.];
define_class!(
    #[unsafe(super(NSView))] #[thread_kind=MainThreadOnly] #[ivars=EditorState]
    struct Editor;
    unsafe impl NSObjectProtocol for Editor {}
    unsafe impl NSWindowDelegate for Editor{
        #[unsafe(method(windowShouldClose:))]
        fn should_close(&self,_:&NSWindow)->bool{self.may_leave()}
    }
    unsafe impl NSControlTextEditingDelegate for Editor{
        #[unsafe(method(controlTextDidChange:))]
        fn text_changed(&self,_:&NSNotification){
            let text=self.ui().text.stringValue().to_string();
            if !document::valid_text(&text){self.ui().text.setStringValue(&s(&self.ivars().work.borrow().doc().text));self.status("يتسع السطر لـ ٢٥٦ محرفًا");return;}
            self.edit(|w|w.doc_mut().edit_text(text));
        }
    }
    unsafe impl NSTextFieldDelegate for Editor {}
    impl Editor{
        #[unsafe(method(scrollWheel:))] fn scroll(&self,e:&NSEvent){if self.ivars().zoom_level.get()>0{let(x,y)=self.ivars().pan.get();self.ivars().pan.set((x+e.scrollingDeltaX(),y-e.scrollingDeltaY()));self.setNeedsDisplay(true);}}
        #[unsafe(method(drawRect:))] fn draw(&self,_:NSRect){self.paint();}
        #[unsafe(method(mouseDown:))] fn down(&self,e:&NSEvent){self.pick(e);}
        #[unsafe(method(mouseDragged:))] fn dragged(&self,e:&NSEvent){self.move_selection(e);}
        #[unsafe(method(mouseUp:))] fn up(&self,_:&NSEvent){self.ivars().drag.set(None);}
        #[unsafe(method(command:))] fn command(&self,sender:&NSControl){self.perform(sender.tag());}
        #[unsafe(method(menuCommand:))] fn menu_command(&self,sender:&NSMenuItem){self.perform(sender.tag());}
    }
);
impl Editor {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(EditorState::default());
        unsafe { msg_send![super(this),initWithFrame:rect(0.,0.,WIDTH,HEIGHT)] }
    }
    fn ui(&self) -> &Controls {
        self.ivars().ui.get().unwrap()
    }
    fn put(&self, view: &NSView, frame: NSRect, anchor: NSAutoresizingMaskOptions) {
        view.setFrame(frame);
        view.setAutoresizingMask(anchor);
        self.addSubview(view);
    }
    fn label(
        &self,
        title: &str,
        size: f64,
        frame: NSRect,
        anchor: NSAutoresizingMaskOptions,
    ) -> Retained<NSTextField> {
        let t = NSTextField::labelWithString(&s(title), self.mtm());
        t.setAlignment(NSTextAlignment::Right);
        t.setFont(Some(&NSFont::systemFontOfSize(size)));
        self.put(&t, frame, anchor);
        t
    }
    fn button(
        &self,
        title: &str,
        action: Sel,
        frame: NSRect,
        anchor: NSAutoresizingMaskOptions,
    ) -> Retained<NSButton> {
        // Every selector used here is implemented above as an AppKit action.
        let b = unsafe {
            NSButton::buttonWithTitle_target_action(&s(title), Some(self), Some(action), self.mtm())
        };
        self.put(&b, frame, anchor);
        b
    }
    fn popup(
        &self,
        action: Sel,
        frame: NSRect,
        anchor: NSAutoresizingMaskOptions,
    ) -> Retained<NSPopUpButton> {
        let p =
            NSPopUpButton::initWithFrame_pullsDown(NSPopUpButton::alloc(self.mtm()), frame, false);
        unsafe {
            p.setTarget(Some(self));
            p.setAction(Some(action));
        }
        self.put(&p, frame, anchor);
        p
    }
    fn setup(&self) {
        use NSAutoresizingMaskOptions as A;
        let right = A::ViewMinXMargin | A::ViewMinYMargin;
        let top = A::ViewMinYMargin;
        let x = WIDTH - SIDE + 16.;
        let w = SIDE - 32.;
        let side = |at: f64, dx: f64, width: f64, h: f64| rect(x + dx, HEIGHT - at - h, width, h);
        self.label("مِداد", 29., side(10., 0., w, 44.), right);
        let b = |name: &str, tag: isize, r: NSRect, anchor| {
            let v = self.button(name, sel!(command:), r, anchor);
            v.setTag(tag);
            v
        };
        let pop = |tag: isize, options: &[&str], r: NSRect, anchor| {
            let v = self.popup(sel!(command:), r, anchor);
            v.setTag(tag);
            for opt in options {
                v.addItemWithTitle(&s(opt));
            }
            v
        };
        let header = |x, w| rect(x, HEIGHT - 50., w, 30.);
        b("جديد", 1, header(14., 56.), top);
        b("فتح…", 2, header(73., 58.), top);
        b("حفظ", 3, header(134., 60.), top);
        let undo = b("تراجع", 4, header(208., 64.), top);
        let redo = b("إعادة", 5, header(274., 64.), top);
        let format = pop(0, &["SVG", "PNG", "PDF"], header(352., 72.), top);
        let export = b("تصدير…", 6, header(429., 80.), top);
        export.setBezelColor(Some(&tint(0.12, 0.38, 0.32)));
        let mode = pop(
            7,
            &[
                "تحريك السطر",
                "تحريك الكلمة",
                "تحريك الحركة",
                "تحريك جزء الحرف",
                "تحريك الحرف",
            ],
            header(528., 148.),
            top,
        );
        let select_page = pop(10, &[], side(81., 0., 168., 28.), right);
        b("صفحة ＋", 11, side(81., 174., w - 174., 28.), right);
        let line = pop(12, &[], side(117., 0., 168., 28.), right);
        b("سطر ＋", 13, side(117., 174., w - 174., 28.), right);
        self.label("نصّ السطر", 15., side(161., 0., w, 23.), right);
        let text = NSTextField::textFieldWithString(&s(document::SAMPLE), self.mtm());
        text.setFont(Some(&NSFont::systemFontOfSize(17.)));
        text.setAlignment(NSTextAlignment::Right);
        text.setBaseWritingDirection(NSWritingDirection::RightToLeft);
        text.setPlaceholderString(Some(&s("اكتب نصّ السطر هنا")));
        unsafe {
            text.setDelegate(Some(ProtocolObject::from_ref(self)));
        }
        self.put(&text, side(190., 0., w, 32.), right);
        let panel = pop(
            14,
            &["الخط وتنسيق السطر", "تركيب الكلمات والحروف", "إعداد الصفحة"],
            side(244., 0., w, 29.),
            right,
        );
        let mut groups: [Vec<Retained<NSView>>; 3] = Default::default();
        // Controls and labels are all NSView subclasses; keep their ordinary hierarchy.
        let add = |groups: &mut [Vec<Retained<NSView>>; 3], i: usize, v: &NSView| {
            groups[i].push(v.retain());
        };
        let entry =
            |groups: &mut [Vec<Retained<NSView>>; 3], i, tag, title: &str, choices: &[&str], y| {
                let lbl = self.label(title, 14., side(y, 185., w - 185., 24.), right);
                add(groups, i, &lbl);
                let p = pop(tag, choices, side(y, 0., 178., 28.), right);
                add(groups, i, &p);
                p
            };
        let fonts = entry(
            &mut groups,
            0,
            20,
            "الخط",
            &Font::available()
                .into_iter()
                .map(Font::name)
                .collect::<Vec<_>>(),
            292.,
        );
        let inks = entry(&mut groups, 0, 21, "لون الحبر", &INKS.map(|v| v.0), 330.);
        let size = entry(&mut groups, 0, 22, "الحجم", &[], 368.);
        let angle = entry(&mut groups, 0, 23, "زاوية السطر", &[], 406.);
        let curve = entry(&mut groups, 0, 24, "قوس الكلمات", &[], 444.);
        let spacing = entry(&mut groups, 0, 25, "تباعد الكلمات", &[], 482.);
        for (title, tag, dx) in [("حفظ أسلوب…", 26, 0.), ("تطبيق أسلوب…", 27, w / 2. + 3.)]
        {
            let v = b(title, tag, side(538., dx, w / 2. - 3., 30.), right);
            add(&mut groups, 0, &v);
        }
        let word = entry(&mut groups, 1, 30, "الكلمة", &[], 292.);
        let forms = entry(&mut groups, 1, 31, "شكل الكلمة", &[], 330.);
        let joins = entry(&mut groups, 1, 32, "الوصلة", &[], 380.);
        let minus = b("تقصير −", 33, side(417., 0., 90., 30.), right);
        let plus = b("مدّ ＋", 34, side(417., 96., 90., 30.), right);
        let split = b("فصل", 35, side(417., 192., w - 192., 30.), right);
        let restore = b(
            "إرجاع الجزء المحدد إلى موضعه",
            36,
            side(474., 0., w, 29.),
            right,
        );
        for v in [&minus, &plus, &split, &restore] {
            add(&mut groups, 1, v);
        }
        let word_ink = entry(&mut groups, 1, 37, "حبر الكلمة", &[], 518.);
        let hint = self.label(
            "اختر أداة التحريك ثم اسحب على اللوحة.",
            13.,
            side(553., 0., w, 25.),
            right,
        );
        add(&mut groups, 1, &hint);
        let paper = entry(&mut groups, 2, 40, "لون الورق", &PAPERS.map(|v| v.0), 292.);
        let frame = b("إضافة إطار", 41, side(335., 0., w, 28.), right);
        let guides = b("إظهار الأدلة", 42, side(375., 0., w, 28.), right);
        guides.setToolTip(Some(&s("تظهر أدلة KLX بخط متقطع. تحدد العلامات الزرقاء مواضع المقاطع، والدوائر الذهبية مواضع النقاط والحركات في السطر المحدد.")));
        for v in [&frame, &guides] {
            add(&mut groups, 2, v);
        }
        for (title, tag, y) in [
            ("تدوير الورقة أفقي / رأسي", 43, 416.),
            ("تكرار السطر", 44, 464.),
            ("حذف السطر", 45, 504.),
            ("حذف الصفحة", 46, 544.),
        ] {
            let v = b(title, tag, side(y, 0., w, 29.), right);
            add(&mut groups, 2, &v);
        }
        b("−", 70, rect(9., 6., 32., 25.), A::empty());
        b("ملاءمة", 71, rect(43., 6., 60., 25.), A::empty());
        b("＋", 72, rect(106., 6., 32., 25.), A::empty());
        let status = self.label(
            "",
            13.,
            rect(150., 8., WIDTH - SIDE - 168., 24.),
            A::ViewWidthSizable,
        );
        status.setTextColor(Some(&NSColor::secondaryLabelColor()));
        self.ivars()
            .ui
            .set(Controls {
                text,
                status,
                page: select_page,
                line,
                fonts,
                inks,
                word_ink,
                size,
                angle,
                curve,
                spacing,
                mode,
                panel,
                joins,
                word,
                forms,
                paper,
                format,
                frame,
                guides,
                minus,
                plus,
                split,
                restore,
                undo,
                redo,
                groups,
            })
            .ok()
            .unwrap();
        self.refresh();
    }
    fn status(&self, message: &str) {
        self.ui().status.setStringValue(&s(message));
    }
    fn checkpoint(&self, state: Work) {
        let mut history = self.ivars().undo.borrow_mut();
        if history.len() >= 48 {
            history.remove(0);
        }
        history.push(state);
        self.ivars().redo.borrow_mut().clear();
    }
    fn edit(&self, f: impl FnOnce(&mut Work)) {
        let old = self.ivars().work.borrow().clone();
        f(&mut self.ivars().work.borrow_mut());
        if old.book != self.ivars().work.borrow().book {
            self.checkpoint(old);
        }
        self.refresh();
    }
    fn history(&self, redo: bool) {
        let state = self.ivars();
        let (from, to) = if redo {
            (&state.redo, &state.undo)
        } else {
            (&state.undo, &state.redo)
        };
        if let Some(next) = from.borrow_mut().pop() {
            to.borrow_mut().push(state.work.replace(next));
            self.clear_selection();
        }
        self.refresh();
    }
    fn clear_selection(&self) {
        self.ivars().selected.replace(None);
        self.ivars().drag.set(None);
    }
    fn refresh(&self) {
        let ui = self.ui();
        let work = self.ivars().work.borrow();
        let page = work.page();
        let doc = work.doc();
        if ui.text.stringValue().to_string() != doc.text {
            ui.text.setStringValue(&s(&doc.text));
        }
        fill(
            &ui.page,
            (1..=work.book.pages.len()).map(|n| format!("صفحة {n}")),
            work.page,
        );
        fill(
            &ui.line,
            page.lines.iter().enumerate().map(|(i, l)| {
                format!(
                    "{} · {}",
                    i + 1,
                    l.content.text.chars().take(16).collect::<String>()
                )
            }),
            work.line,
        );
        ui.fonts.selectItemAtIndex(
            Font::available()
                .iter()
                .position(|v| *v == doc.style.font)
                .unwrap() as isize,
        );
        let custom = doc
            .style
            .custom_ink
            .map(|c| format!("حبر خاص {}", crate::book::hex_color(c)));
        let color_index = if custom.is_some() {
            INKS.len()
        } else {
            doc.style.ink
        };
        fill(
            &ui.inks,
            INKS.iter().map(|v| v.0.to_owned()).chain(custom),
            color_index,
        );
        ui.paper.selectItemAtIndex(page.paper as isize);
        for (p, choices, value, unit, factor) in [
            (&ui.size, SIZES, doc.style.size, "٪", 100.),
            (&ui.angle, ANGLES, doc.style.angle, "°", 1.),
            (&ui.curve, CURVES, doc.style.curve, "", 1.),
            (&ui.spacing, SPACES, doc.style.spacing, "", 1.),
        ] {
            let mut values = choices.to_vec();
            let i = values.iter().position(|&n| n == value).unwrap_or_else(|| {
                values.push(value);
                values.len() - 1
            });
            fill(
                p,
                values.iter().map(|v| format!("{:.0}{unit}", v * factor)),
                i,
            );
        }
        let words = document::words(&doc.text);
        let selected_word = match self.ivars().selected.borrow().as_ref() {
            Some(Target::Word(k)) => words.iter().position(|(i, _)| i == k),
            _ => None,
        };
        let wi = selected_word
            .unwrap_or_else(|| index(&ui.word))
            .min(words.len().saturating_sub(1));
        fill(
            &ui.word,
            words
                .iter()
                .enumerate()
                .map(|(n, (_, w))| format!("{} · {w}", n + 1)),
            wi,
        );
        let mut labels: Vec<String> = std::iter::once("حبر السطر")
            .chain(INKS.iter().map(|v| v.0))
            .map(str::to_owned)
            .collect();
        let mut ink_index = 0;
        if let Some((start, word)) = words.get(wi) {
            let mut colors = word
                .char_indices()
                .map(|(i, _)| doc.letter_inks.get(&(start + i)));
            let first = colors.next().flatten();
            if !colors.all(|c| c == first) {
                ink_index = labels.len();
                labels.push("ألوان متعددة".into());
            } else if let Some(color) = first {
                let hex = crate::book::hex_color(*color);
                ink_index = INKS
                    .iter()
                    .position(|v| v.1 == hex)
                    .map(|i| i + 1)
                    .unwrap_or_else(|| {
                        labels.push(hex);
                        labels.len() - 1
                    });
            }
        }
        fill(&ui.word_ink, labels.into_iter(), ink_index);
        ui.word_ink.setEnabled(!words.is_empty());
        let forms = if index(&ui.panel) == 1 {
            words
                .get(wi)
                .map(|(k, _)| crate::scene::available_forms(doc, *k))
                .unwrap_or_default()
        } else {
            vec![]
        };
        self.ivars().form_choices.replace(forms.clone());
        let fi = words
            .get(wi)
            .and_then(|(k, _)| doc.forms.get(k))
            .and_then(|f| forms.iter().position(|(tag, _)| tag == f))
            .map_or(0, |i| i + 1);
        fill(
            &ui.forms,
            std::iter::once("تلقائي".into())
                .chain(forms.iter().map(|(_, label)| label.to_string())),
            fi,
        );
        ui.forms.setEnabled(!words.is_empty() && !forms.is_empty());
        let joins = document::joins(&doc.text);
        let ji = index(&ui.joins).min(joins.len().saturating_sub(1));
        fill(
            &ui.joins,
            joins.iter().enumerate().map(|(n, (i, p))| {
                format!(
                    "{} · {p} · {}",
                    n + 1,
                    doc.stretches.get(i).copied().unwrap_or(0)
                )
            }),
            ji,
        );
        let joint = joins.get(ji).map(|(k, _)| *k);
        let count = joint
            .and_then(|k| doc.stretches.get(&k))
            .copied()
            .unwrap_or(0);
        let split = joint.is_some_and(|k| doc.breaks.contains(&k));
        ui.minus.setEnabled(!split && count > 0);
        ui.plus
            .setEnabled(joint.is_some() && !split && count < document::MAX_STRETCH);
        ui.split.setEnabled(joint.is_some());
        ui.split.setTitle(&s(if split { "وصل" } else { "فصل" }));
        ui.frame.setTitle(&s(if page.border {
            "إزالة الإطار"
        } else {
            "إضافة إطار"
        }));
        ui.guides.setTitle(&s(if self.ivars().show_guides.get() {
            "إخفاء الأدلة"
        } else {
            "إظهار الأدلة"
        }));
        ui.undo.setEnabled(!self.ivars().undo.borrow().is_empty());
        ui.redo.setEnabled(!self.ivars().redo.borrow().is_empty());
        for (i, views) in ui.groups.iter().enumerate() {
            for view in views {
                view.setHidden(i != index(&ui.panel));
            }
        }
        ui.restore
            .setEnabled(self.ivars().selected.borrow().is_some());
        if let Some(window) = self.window() {
            window.setDocumentEdited(work.book != *self.ivars().saved.borrow());
        }
        self.ivars().board.replace(Some(Board::new(page)));
        self.status(&format!(
            "صفحة {} من {}   ·   سطر {} من {}   ·   {}",
            work.page + 1,
            work.book.pages.len(),
            work.line + 1,
            page.lines.len(),
            doc.style.font.name()
        ));
        self.refresh_image();
    }
    fn refresh_image(&self) {
        let work = self.ivars().work.borrow();
        let board = self.ivars().board.borrow();
        let selected = self.ivars().selected.borrow();
        let svg = board.as_ref().unwrap().svg(
            work.page(),
            selected.as_ref().map(|t| (work.line, t)),
            self.ivars().show_guides.get().then_some(work.line),
        );
        let image = NSImage::initWithData(NSImage::alloc(), &NSData::with_bytes(svg.as_bytes()));
        if image.is_none() {
            self.status("تعذّر رسم المعاينة");
        }
        self.ivars().image.replace(image);
        self.setNeedsDisplay(true);
    }
    fn paper_rect(&self, p: &Page) -> (NSRect, f64) {
        let size = self.bounds().size;
        let w = size.width - SIDE;
        let h = size.height - HEADER - 38.;
        let scale = ((w - 48.) / p.width).min((h - 40.) / p.height)
            * 1.35f64.powi(self.ivars().zoom_level.get());
        let (x, y) = self.ivars().pan.get();
        let x = x.clamp(-p.width * scale / 2., p.width * scale / 2.);
        let y = y.clamp(-p.height * scale / 2., p.height * scale / 2.);
        (
            rect(
                (w - p.width * scale) / 2. + x,
                38. + (h - p.height * scale) / 2. + y,
                p.width * scale,
                p.height * scale,
            ),
            scale,
        )
    }
    fn paint(&self) {
        let size = self.bounds().size;
        NSColor::windowBackgroundColor().setFill();
        NSBezierPath::fillRect(self.bounds());
        tint(0.875, 0.897, 0.878).setFill();
        NSBezierPath::fillRect(rect(0., 38., size.width - SIDE, size.height - HEADER - 38.));
        NSColor::separatorColor().setFill();
        NSBezierPath::fillRect(rect(size.width - SIDE, 0., 1., size.height - HEADER));
        NSBezierPath::fillRect(rect(0., size.height - HEADER, size.width, 1.));
        if let Some(image) = self.ivars().image.borrow().as_ref() {
            let paper = self.paper_rect(self.ivars().work.borrow().page()).0;
            NSGraphicsContext::saveGraphicsState_class();
            NSBezierPath::clipRect(rect(0., 38., size.width - SIDE, size.height - HEADER - 38.));
            image.drawInRect(paper);
            NSGraphicsContext::restoreGraphicsState_class();
        }
    }
    fn pick(&self, e: &NSEvent) {
        let p = self.convertPoint_fromView(e.locationInWindow(), None);
        let (page, scale) = self.paper_rect(self.ivars().work.borrow().page());
        let x = (p.x - page.origin.x) / scale;
        let y = (page.origin.y + page.size.height - p.y) / scale;
        let hit = self
            .ivars()
            .board
            .borrow()
            .as_ref()
            .and_then(|b| b.hit(x, y, index(&self.ui().mode), 4. / scale));
        self.clear_selection();
        if let Some((line, t)) = hit {
            self.ivars().work.borrow_mut().line = line;
            self.ivars().selected.replace(Some(t));
            self.ivars().drag.set(Some(p));
            self.ivars().drag_started.set(false);
        }
        self.refresh();
    }
    fn move_selection(&self, e: &NSEvent) {
        let state = self.ivars();
        let Some(prev) = state.drag.get() else { return };
        let Some(t) = state.selected.borrow().clone() else {
            return;
        };
        let p = self.convertPoint_fromView(e.locationInWindow(), None);
        if p == prev {
            return;
        }
        if !state.drag_started.replace(true) {
            self.checkpoint(state.work.borrow().clone());
        }
        {
            let mut w = state.work.borrow_mut();
            let view_scale = self.paper_rect(w.page()).1;
            let (dx, dy) = ((p.x - prev.x) / view_scale, (prev.y - p.y) / view_scale);
            let line = w.line;
            let frame = state.board.borrow().as_ref().unwrap().lines[line].frame;
            let page = w.page_mut();
            let layer = &mut page.lines[line];
            if t == Target::Line {
                layer.x = (layer.x + dx / page.width).clamp(0., 1.);
                layer.y = (layer.y + dy / page.height).clamp(0., 1.);
            } else {
                let (dx, dy) = frame.inverse_delta(dx, dy);
                let doc = &mut layer.content;
                let (off, bound) = match t {
                    Target::Mark(k) => (doc.marks.entry(k).or_default(), 60.),
                    Target::Part(k) => (doc.parts.entry(k).or_default(), 100.),
                    Target::Letter(k) => (doc.letters.entry(k).or_default(), 500.),
                    Target::Word(k) => (doc.words.entry(k).or_default(), document::MAX_WORD_OFFSET),
                    Target::Line => unreachable!(),
                };
                off.x = (off.x + dx).clamp(-bound, bound);
                off.y = (off.y + dy).clamp(-bound, bound);
            }
        }
        state.drag.set(Some(p));
        self.refresh();
    }
    fn perform(&self, tag: isize) {
        let ui = self.ui();
        let state = self.ivars();
        match tag {
            0 => {}
            1 => {
                if self.may_leave() {
                    self.replace_work(Book::default(), None, false);
                }
            }
            2 => self.open(false),
            3 => {
                self.save();
            }
            4 | 5 => self.history(tag == 5),
            6 | 60 | 61 | 62 => {
                let i = if tag == 6 {
                    index(&ui.format)
                } else {
                    (tag - 60) as usize
                };
                let ext = ["svg", "png", "pdf"][i];
                if let Some(path) = self.destination(ext) {
                    let result = {
                        let w = state.work.borrow();
                        output::export(&w.book, w.page, ext, &path)
                    };
                    match result {
                        Ok(()) => self.status(&format!(
                            "تم التصدير · {}",
                            path.file_name().unwrap_or_default().to_string_lossy()
                        )),
                        Err(e) => self.error(&e),
                    }
                }
            }
            7 => {
                self.clear_selection();
                self.refresh();
            }
            10 | 12 => {
                let mut w = state.work.borrow_mut();
                if tag == 10 {
                    w.page = index(&ui.page);
                    w.line = 0;
                } else {
                    w.line = index(&ui.line);
                }
                drop(w);
                self.clear_selection();
                self.refresh();
            }
            11 => {
                if state.work.borrow().book.pages.len() >= MAX_PAGES {
                    self.status("بلغ المشروع الحد المتاح للصفحات");
                    return;
                }
                self.clear_selection();
                self.edit(|w| {
                    let mut p = Page::default();
                    p.lines[0].content.edit_text(String::new());
                    w.book.pages.push(p);
                    w.page = w.book.pages.len() - 1;
                    w.line = 0;
                });
            }
            13 | 44 => {
                if state.work.borrow().page().lines.len() >= MAX_LINES {
                    self.status("بلغت الصفحة الحد المتاح للأسطر");
                    return;
                }
                self.clear_selection();
                self.edit(|w| {
                    let mut line = w.page().lines[w.line].clone();
                    if tag == 13 {
                        line.content.edit_text(String::new());
                    }
                    line.baseline = None;
                    line.reference = None;
                    line.y = if line.y + 0.19 <= 0.92 {
                        line.y + 0.19
                    } else {
                        0.15
                    };
                    w.page_mut().lines.push(line);
                    w.line = w.page().lines.len() - 1;
                });
                if tag == 13 {
                    if let Some(window) = self.window() {
                        window.makeFirstResponder(Some(&ui.text));
                    }
                }
            }
            14 | 32 => self.refresh(),
            20 => {
                self.clear_selection();
                self.edit(|w| {
                    profile::change_font(w.doc_mut(), Font::available()[index(&ui.fonts)])
                });
            }
            21 => {
                let i = index(&ui.inks);
                if i < INKS.len() {
                    self.edit(|w| {
                        w.doc_mut().style.ink = i;
                        w.doc_mut().style.custom_ink = None;
                    });
                }
            }
            37 => {
                let i = index(&ui.word_ink);
                if i <= INKS.len() {
                    let color = i.checked_sub(1).map(|j| {
                        let [_, r, g, b] = u32::from_str_radix(&INKS[j].1[1..], 16)
                            .expect("palette RGB")
                            .to_be_bytes();
                        [r, g, b]
                    });
                    self.edit(|w| w.doc_mut().set_word_ink(index(&ui.word), color));
                }
            }
            22..=25 => self.edit(|w| {
                let style = &mut w.doc_mut().style;
                let (current, values, p) = match tag {
                    22 => (&mut style.size, SIZES, &ui.size),
                    23 => (&mut style.angle, ANGLES, &ui.angle),
                    24 => (&mut style.curve, CURVES, &ui.curve),
                    _ => (&mut style.spacing, SPACES, &ui.spacing),
                };
                if let Some(&n) = values.get(index(p)) {
                    *current = n;
                }
            }),
            26 => {
                if let Some(path) = self.destination("midadstyle") {
                    let result = profile::save(&path, state.work.borrow().doc());
                    match result {
                        Ok(()) => self.status("تم حفظ أسلوب الخطاط"),
                        Err(e) => self.error(&e),
                    }
                }
            }
            27 => {
                if let Some(path) = self.choose_file("midadstyle") {
                    match profile::load(&path) {
                        Ok(model) => {
                            self.clear_selection();
                            self.edit(|w| profile::apply(w.doc_mut(), &model));
                            self.status("طُبّق الأسلوب على السطر والكلمات المطابقة");
                        }
                        Err(e) => self.error(&e),
                    }
                }
            }
            30 | 31 => {
                let key = document::words(&state.work.borrow().doc().text)
                    .get(index(&ui.word))
                    .map(|v| v.0);
                if let Some(k) = key {
                    state.selected.replace(Some(Target::Word(k)));
                    ui.mode.selectItemAtIndex(1);
                    if tag == 30 {
                        self.refresh();
                    } else {
                        let f = index(&ui.forms);
                        self.edit(|w| {
                            let doc = w.doc_mut();
                            if f == 0 {
                                doc.forms.remove(&k);
                            } else if let Some((v, _)) = state.form_choices.borrow().get(f - 1) {
                                doc.forms.insert(k, v.to_string());
                            }
                        });
                    }
                }
            }
            33..=35 => {
                let key = document::joins(&state.work.borrow().doc().text)
                    .get(index(&ui.joins))
                    .map(|v| v.0);
                if let Some(k) = key {
                    self.edit(|w| {
                        let doc = w.doc_mut();
                        if tag == 35 {
                            if !doc.breaks.remove(&k) {
                                doc.breaks.insert(k);
                            }
                            return;
                        }
                        let old = doc.stretches.get(&k).copied().unwrap_or(0);
                        let next = if tag == 34 {
                            old.saturating_add(1).min(document::MAX_STRETCH)
                        } else {
                            old.saturating_sub(1)
                        };
                        if next == 0 {
                            doc.stretches.remove(&k);
                        } else {
                            doc.stretches.insert(k, next);
                        }
                    });
                }
            }
            36 => {
                let target = state.selected.borrow().clone();
                self.edit(|w| {
                    let i = w.line;
                    let l = &mut w.page_mut().lines[i];
                    match target {
                        Some(Target::Line) => {
                            l.x = 0.5;
                            l.y = 0.5;
                        }
                        Some(Target::Mark(k)) => {
                            l.content.marks.remove(&k);
                        }
                        Some(Target::Part(k)) => {
                            l.content.parts.remove(&k);
                        }
                        Some(Target::Letter(k)) => {
                            l.content.letters.remove(&k);
                        }
                        Some(Target::Word(k)) => {
                            l.content.words.remove(&k);
                        }
                        None => {}
                    }
                });
            }
            40 => self.edit(|w| w.page_mut().paper = index(&ui.paper)),
            41 => self.edit(|w| w.page_mut().border = !w.page().border),
            42 => {
                let visible = !state.show_guides.get();
                state.show_guides.set(visible);
                self.refresh();
                let work = state.work.borrow();
                let guides = &work.page().source_guides;
                if !visible {
                    self.status("الأدلة مخفية");
                } else if !guides.is_empty() {
                    let circles = guides
                        .iter()
                        .filter(|g| g.kind == crate::book::SourceGuide::CIRCLE)
                        .count();
                    let ellipses = guides
                        .iter()
                        .filter(|g| g.kind == crate::book::SourceGuide::ELLIPSE)
                        .count();
                    self.status(&format!("أدلة KLX · دائرية {circles} · بيضاوية {ellipses}"));
                } else {
                    self.status("أدلة الصفحة ظاهرة");
                }
            }
            43 => self.edit(|w| {
                let p = w.page_mut();
                std::mem::swap(&mut p.width, &mut p.height);
            }),
            45 | 46 => {
                self.clear_selection();
                self.edit(|w| {
                    if tag == 45 {
                        let i = w.line;
                        let p = w.page_mut();
                        if p.lines.len() > 1 {
                            p.lines.remove(i);
                            w.line = i.min(p.lines.len() - 1);
                        } else {
                            p.lines[0].content.edit_text(String::new());
                            p.lines[0].baseline = None;
                            p.lines[0].reference = None;
                        }
                    } else if w.book.pages.len() > 1 {
                        w.book.pages.remove(w.page);
                        w.page = w.page.min(w.book.pages.len() - 1);
                        w.line = 0;
                    } else {
                        let mut p = Page::default();
                        p.lines[0].content.edit_text(String::new());
                        w.book.pages[0] = p;
                        w.line = 0;
                    }
                });
            }
            47 => {
                let panel = NSOpenPanel::openPanel(self.mtm());
                #[allow(deprecated)]
                panel.setAllowedFileTypes(Some(&NSArray::from_retained_slice(&[
                    s("png"),
                    s("jpg"),
                    s("jpeg"),
                ])));
                if panel.runModal() == 1
                    && let Some(path) = panel.URL().and_then(|u| u.to_file_path())
                {
                    match crate::backdrop::load(&path) {
                        Ok(image) => self.edit(|w| w.page_mut().background = Some(image)),
                        Err(e) => self.error(&e),
                    }
                }
            }
            78 => {
                let pos = ui
                    .text
                    .currentEditor()
                    .map(|e| e.selectedRange())
                    .map_or(0, |r| r.location.saturating_add(r.length));
                let hit = {
                    let w = state.work.borrow();
                    w.book.next_unread((w.page, w.line, pos))
                };
                if let Some((p, l, offset)) = hit {
                    {
                        let mut w = state.work.borrow_mut();
                        w.page = p;
                        w.line = l;
                    }
                    self.clear_selection();
                    self.refresh();
                    if let Some(window) = self.window() {
                        window.makeFirstResponder(Some(&ui.text));
                        if let Some(editor) = ui.text.currentEditor() {
                            editor.setSelectedRange(objc2_foundation::NSRange::new(offset, 1));
                        }
                    }
                    self.status(&format!(
                        "صفحة {} · سطر {} · اكتب الحرف مكان الرمز المحدد",
                        p + 1,
                        l + 1
                    ));
                } else {
                    self.status("اكتملت المراجعة، كل رموز اللوحة مقروءة");
                }
            }
            77 => {
                if state.work.borrow().page().pictures.is_empty() {
                    self.status("تخلو الصفحة من الزخارف المستوردة");
                } else {
                    self.edit(|w| {
                        let p = w.page_mut();
                        p.hide_pictures = !p.hide_pictures;
                    });
                }
            }
            48 => self.edit(|w| w.page_mut().background = None),
            49 => {
                let result = output::encode(&state.work.borrow().book, 0, "pdf");
                match result {
                    Ok(bytes) => {
                        use objc2_pdf_kit::{PDFDocument, PDFPrintScalingMode};
                        let pdf = unsafe {
                            PDFDocument::initWithData(
                                PDFDocument::alloc(),
                                &NSData::with_bytes(&bytes),
                            )
                        };
                        let operation = pdf.and_then(|p| unsafe {
                            p.printOperationForPrintInfo_scalingMode_autoRotate(
                                None,
                                PDFPrintScalingMode::PageScaleToFit,
                                true,
                                self.mtm(),
                            )
                        });
                        if let Some(op) = operation {
                            op.runOperation();
                        } else {
                            self.error("تعذر إعداد معاينة الطباعة");
                        }
                    }
                    Err(e) => self.error(&e),
                }
            }
            70..=72 => {
                let v = if tag == 71 {
                    0
                } else {
                    (state.zoom_level.get() + if tag == 70 { -1 } else { 1 }).clamp(-2, 5)
                };
                state.zoom_level.set(v);
                if v <= 0 {
                    state.pan.set((0., 0.));
                }
                self.setNeedsDisplay(true);
                self.status(&format!("تكبير {:.0}٪", 100. * 1.35f64.powi(v)));
            }
            73 => {
                state.selected.replace(Some(Target::Line));
                self.perform(36);
            }
            74 | 75 => {
                state.show_guides.set(true);
                self.edit(|w| {
                    let l = &w.page().lines[w.line];
                    let guide = (tag == 75, if tag == 75 { l.x } else { l.y }.clamp(0., 1.));
                    let page = w.page_mut();
                    if !page.guides.contains(&guide) && page.guides.len() < 128 {
                        page.guides.push(guide);
                    }
                });
            }
            76 => self.edit(|w| w.page_mut().guides.clear()),
            50 => self.open(true),
            51 => {
                if let Some(path) = self.destination("midad") {
                    self.save_to(&path);
                }
            }
            _ => {}
        }
    }
    #[allow(deprecated)]
    fn choose_file(&self, ext: &str) -> Option<PathBuf> {
        let panel = NSOpenPanel::openPanel(self.mtm());
        panel.setAllowedFileTypes(Some(&NSArray::from_retained_slice(&[s(ext)])));
        (panel.runModal() == 1)
            .then(|| panel.URL()?.to_file_path())
            .flatten()
    }
    fn replace_work(&self, book: Book, path: Option<&Path>, imported: bool) {
        let state = self.ivars();
        state.saved.replace(if imported {
            Book::default()
        } else {
            book.clone()
        });
        state.work.replace(Work {
            book,
            page: 0,
            line: 0,
        });
        state
            .path
            .replace(path.filter(|_| !imported).map(Path::to_path_buf));
        state.undo.borrow_mut().clear();
        state.redo.borrow_mut().clear();
        self.clear_selection();
        state.zoom_level.set(0);
        state.pan.set((0., 0.));
        state.show_guides.set(imported);
        self.title();
        self.refresh();
        if imported {
            if let (Some(window), Some(file)) = (self.window(), path.and_then(Path::file_name)) {
                window.setTitle(&s(&format!("{} — مِداد", file.to_string_lossy())));
            }
            let work = state.work.borrow();
            let lines = work.book.pages.iter().flat_map(|p| &p.lines);
            let restored = lines
                .clone()
                .filter(|l| !l.content.text.trim().is_empty())
                .count();
            let unknown = lines
                .flat_map(|l| l.content.text.chars())
                .filter(|&c| c == '�')
                .count();
            let mut message = format!("استُورد {restored} سطرًا بنص قابل للتحرير");
            if unknown > 0 {
                message.push_str(&format!(" · رموز تحتاج مراجعة {unknown}"));
            }
            let pictures: usize = work.book.pages.iter().map(|p| p.pictures.len()).sum();
            let missing: usize = work.book.pages.iter().map(|p| p.missing_pictures).sum();
            if pictures > 0 {
                message.push_str(&format!(" · زخارف {pictures}"));
            }
            if missing > 0 {
                message.push_str(&format!(" · صور تعذّر فتحها {missing}"));
            }
            self.status(&message);
        }
    }
    fn open(&self, import: bool) {
        let Some(path) = self.choose_file(if import { "klx" } else { "midad" }) else {
            return;
        };
        self.open_path(&path, import);
    }
    fn open_path(&self, path: &Path, import: bool) -> bool {
        if !self.may_leave() {
            return false;
        }
        match if import {
            klx::load(path)
        } else {
            Book::load(path)
        } {
            Ok(book) => {
                self.replace_work(book, Some(path), import);
                true
            }
            Err(e) => {
                self.error(&e);
                false
            }
        }
    }
    fn title(&self) {
        if let Some(window) = self.window() {
            let title = self
                .ivars()
                .path
                .borrow()
                .as_ref()
                .map(|p| {
                    format!(
                        "{} — مِداد",
                        p.file_stem().unwrap_or_default().to_string_lossy()
                    )
                })
                .unwrap_or_else(|| "مِداد".into());
            window.setTitle(&s(&title));
        }
    }
    fn save_to(&self, path: &Path) -> bool {
        let result = self.ivars().work.borrow().book.save(path);
        match result {
            Ok(()) => {
                self.ivars()
                    .saved
                    .replace(self.ivars().work.borrow().book.clone());
                self.ivars().path.replace(Some(path.into()));
                self.title();
                self.refresh();
                self.status("تم حفظ المشروع");
                true
            }
            Err(e) => {
                self.error(&e);
                false
            }
        }
    }
    fn save(&self) -> bool {
        let path = self
            .ivars()
            .path
            .borrow()
            .clone()
            .or_else(|| self.destination("midad"));
        path.is_some_and(|p| self.save_to(&p))
    }
    fn error(&self, detail: &str) {
        let a = NSAlert::new(self.mtm());
        a.setMessageText(&s("تعذّر إتمام العملية"));
        a.setInformativeText(&s(detail));
        a.runModal();
    }
    fn may_leave(&self) -> bool {
        if self.ivars().work.borrow().book == *self.ivars().saved.borrow() {
            return true;
        }
        let a = NSAlert::new(self.mtm());
        a.setMessageText(&s("هل تريد حفظ التعديلات؟"));
        for title in ["حفظ", "إلغاء", "تجاهل التعديلات"] {
            a.addButtonWithTitle(&s(title));
        }
        match a.runModal() {
            1000 => self.save(),
            1002 => true,
            _ => false,
        }
    }
    #[allow(deprecated)]
    fn destination(&self, ext: &str) -> Option<PathBuf> {
        let panel = NSSavePanel::savePanel(self.mtm());
        panel.setAllowedFileTypes(Some(&NSArray::from_retained_slice(&[s(ext)])));
        panel.setNameFieldStringValue(&s(&format!("لوحة.{ext}")));
        (panel.runModal() == 1)
            .then(|| panel.URL()?.to_file_path())
            .flatten()
    }
}
fn index(p: &NSPopUpButton) -> usize {
    p.indexOfSelectedItem().max(0) as usize
}
fn fill(p: &NSPopUpButton, labels: impl Iterator<Item = String>, selected: usize) {
    p.removeAllItems();
    for label in labels {
        p.addItemWithTitle(&s(&label));
    }
    let count = p.numberOfItems();
    p.setEnabled(count > 0);
    if count > 0 {
        p.selectItemAtIndex((selected as isize).min(count - 1));
    }
}
#[derive(Default)]
struct AppState {
    pending: RefCell<Option<PathBuf>>,
    window: OnceCell<Retained<NSWindow>>,
    editor: OnceCell<Retained<Editor>>,
}
define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind=MainThreadOnly]
    #[ivars=AppState]
    struct AppDelegate;
    unsafe impl NSObjectProtocol for AppDelegate {}
    unsafe impl NSApplicationDelegate for AppDelegate {
        #[unsafe(method(application:openFile:))]
        fn open_file(&self, _: &NSApplication, filename: &NSString) -> bool {
            let path = PathBuf::from(filename.to_string());
            let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
            let import = ext.eq_ignore_ascii_case("klx");
            if !import && !ext.eq_ignore_ascii_case("midad") {
                false
            } else if let Some(editor) = self.ivars().editor.get() {
                editor.open_path(&path, import)
            } else if self.ivars().pending.borrow().is_none() {
                self.ivars().pending.replace(Some(path));
                true
            } else {
                false
            }
        }
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn launch(&self, _: &NSNotification) {
            let mtm = self.mtm();
            let app = NSApplication::sharedApplication(mtm);
            let style = NSWindowStyleMask::Titled
                | NSWindowStyleMask::Closable
                | NSWindowStyleMask::Miniaturizable
                | NSWindowStyleMask::Resizable;
            let window = unsafe {
                NSWindow::initWithContentRect_styleMask_backing_defer(
                    NSWindow::alloc(mtm),
                    rect(0., 0., WIDTH, HEIGHT),
                    style,
                    NSBackingStoreType::Buffered,
                    false,
                )
            };
            unsafe {
                window.setReleasedWhenClosed(false);
                window.setContentMinSize(NSSize::new(1030., 620.));
            }
            let editor = Editor::new(mtm);
            window.setContentView(Some(&editor));
            window.setDelegate(Some(ProtocolObject::from_ref(&*editor)));
            window.setTitle(&s("مِداد"));
            editor.setup();
            window.center();
            window.makeKeyAndOrderFront(None);
            menus(&app, &editor);
            if let Some(path) = self.ivars().pending.take() {
                let import = path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("klx"));
                editor.open_path(&path, import);
            }
            self.ivars().editor.set(editor).ok().unwrap();
            self.ivars().window.set(window).ok().unwrap();
            #[allow(deprecated)]
            app.activateIgnoringOtherApps(true);
        }
        #[unsafe(method(applicationShouldTerminate:))]
        fn terminate(&self, _: &NSApplication) -> NSApplicationTerminateReply {
            if self.ivars().window.get().is_some_and(|w| w.isVisible())
                && self.ivars().editor.get().is_some_and(|e| !e.may_leave())
            {
                NSApplicationTerminateReply::TerminateCancel
            } else {
                NSApplicationTerminateReply::TerminateNow
            }
        }
        #[unsafe(method(applicationShouldTerminateAfterLastWindowClosed:))]
        fn last_window(&self, _: &NSApplication) -> bool {
            true
        }
    }
);
fn menus(app: &NSApplication, editor: &Editor) {
    let mtm = editor.mtm();
    let bar = NSMenu::new(mtm);
    for (title, items) in [
        ("مِداد", vec![("إنهاء مداد", sel!(terminate:), "q", 0)]),
        (
            "ملف",
            vec![
                ("جديد", sel!(menuCommand:), "n", 1),
                ("فتح…", sel!(menuCommand:), "o", 2),
                ("استيراد ملف KLX…", sel!(menuCommand:), "", 50),
                ("حفظ", sel!(menuCommand:), "s", 3),
                ("حفظ باسم…", sel!(menuCommand:), "", 51),
                ("تصدير الصفحة إلى SVG…", sel!(menuCommand:), "e", 60),
                ("تصدير الصفحة إلى PNG…", sel!(menuCommand:), "", 61),
                ("تصدير الصفحات إلى PDF…", sel!(menuCommand:), "", 62),
                ("طباعة…", sel!(menuCommand:), "p", 49),
            ],
        ),
        (
            "تحرير",
            vec![
                ("تراجع", sel!(menuCommand:), "z", 4),
                ("إعادة", sel!(menuCommand:), "Z", 5),
                ("قصّ", sel!(cut:), "x", 0),
                ("نسخ", sel!(copy:), "c", 0),
                ("لصق", sel!(paste:), "v", 0),
                (
                    "مراجعة الرمز غير المقروء التالي",
                    sel!(menuCommand:),
                    "g",
                    78,
                ),
                ("تحديد الكل", sel!(selectAll:), "a", 0),
            ],
        ),
        (
            "تركيب",
            vec![
                ("إضافة سطر", sel!(menuCommand:), "", 13),
                ("تكرار السطر", sel!(menuCommand:), "", 44),
                ("توسيط السطر", sel!(menuCommand:), "", 73),
                ("إضافة صفحة", sel!(menuCommand:), "", 11),
                ("أدلة الصفحة", sel!(menuCommand:), "", 42),
                ("دليل أفقي عند السطر", sel!(menuCommand:), "", 74),
                ("دليل عمودي عند السطر", sel!(menuCommand:), "", 75),
                ("مسح الأدلة المضافة", sel!(menuCommand:), "", 76),
                ("إدراج صورة خلفية…", sel!(menuCommand:), "", 47),
                ("إظهار أو إخفاء زخارف KLX", sel!(menuCommand:), "", 77),
                ("إزالة الصورة الخلفية", sel!(menuCommand:), "", 48),
                ("حفظ أسلوب الخطاط…", sel!(menuCommand:), "", 26),
                ("تطبيق أسلوب الخطاط…", sel!(menuCommand:), "", 27),
            ],
        ),
    ] {
        let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &s(title));
        let head = NSMenuItem::new(mtm);
        bar.addItem(&head);
        head.setSubmenu(Some(&menu));
        for (label, action, key, target) in items {
            unsafe {
                let item =
                    menu.addItemWithTitle_action_keyEquivalent(&s(label), Some(action), &s(key));
                if target != 0 {
                    item.setTag(target);
                    item.setTarget(Some(editor));
                }
            }
        }
    }
    app.setMainMenu(Some(&bar));
}
pub fn run() {
    let mtm = MainThreadMarker::new().expect("main thread");
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    let delegate: Retained<AppDelegate> = unsafe {
        let d = AppDelegate::alloc(mtm).set_ivars(AppState::default());
        msg_send![super(d), init]
    };
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    app.run();
}

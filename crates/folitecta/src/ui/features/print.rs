//! The print window: the printer and its settings on the left, a live
//! preview of the sheets as they will print on the right. Opening it never
//! prints: nothing is sent before Print, Save or System dialog is pressed.
//! The sheets are planned at once from the settings; the preview builds the
//! print file of the sheet shown (one sheet, so it is quick for any document)
//! and draws it.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use dioxus::prelude::*;
use folitecta_core::print::{
    self, Binding, BookletSides, BuildOptions, Content, Face, Layout, NupOrder, Orientation, PageGeom, Paper,
    PlanError, PlanWarning, PrintOptions, PrintPermission, PrintPlan, PrintSeed, PrintSource,
    STANDARD_PAPERS, Sides, Sizing, Subset,
};
use serde::{Deserialize, Serialize};

use crate::commands::print::{self as runner, Destination, JOB, PrintItem, PrintRequest};
use crate::i18n::{Lang, t, tf};
use crate::icons::Icon;
use crate::printing::{self, JobTicket, Media, PrintError, PrinterCaps, PrinterInfo, PrinterState};
use crate::state::{AppState, Dialog, DocTab, PrintTarget};
use crate::ui::dialogs::Frame;
use crate::util::{RangeError, format_pages, parse_print_range};

/// Where the settings are kept between uses.
pub const PREFS_KEY: &str = "print.settings";

const PDF: &str = "pdf";
const SYSTEM: &str = "system";
const MM: f32 = 72.0 / 25.4;

/// How the pages are arranged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Size,
    Multiple,
    Booklet,
    Poster,
}

/// The size of single pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SizeChoice {
    #[default]
    Fit,
    Actual,
    Shrink,
    Custom,
}

/// The settings of one printer.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PrinterPrefs {
    pub paper: String,
    pub sides: Option<Sides>,
    pub gray: Option<bool>,
}

/// The settings of the window, remembered (copies are not).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PrintPrefs {
    /// A printer's name, `pdf` or `system`.
    pub destination: String,
    pub collate: bool,
    pub content: Content,
    pub mode: Mode,
    pub size: SizeChoice,
    /// Custom scale, percent.
    pub scale: f32,
    pub paper_by_page: bool,
    pub auto_rotate: bool,
    pub per_sheet: u8,
    pub order: NupOrder,
    pub gutter_mm: f32,
    pub borders: bool,
    pub binding: Binding,
    pub booklet_sides: BookletSides,
    pub booklet_sheets: u16,
    pub back_cover: bool,
    pub poster_scale: f32,
    pub overlap_mm: f32,
    pub cut_marks: bool,
    pub labels: bool,
    pub orientation: Orientation,
    pub subset: Subset,
    pub reverse: bool,
    pub as_image: bool,
    pub image_dpi: u32,
    /// Windows: the resolution of the pictures (0: the printer's, at most 600).
    pub win_dpi: u32,
    pub printers: HashMap<String, PrinterPrefs>,
    /// The paper for saving as PDF and the system dialog.
    pub pdf_paper: String,
}

impl Default for PrintPrefs {
    fn default() -> Self {
        PrintPrefs {
            destination: String::new(),
            collate: true,
            content: Content::DocumentAndComments,
            mode: Mode::Size,
            size: SizeChoice::Fit,
            scale: 100.0,
            paper_by_page: false,
            auto_rotate: true,
            per_sheet: 2,
            order: NupOrder::Horizontal,
            gutter_mm: 0.0,
            borders: false,
            binding: Binding::Left,
            booklet_sides: BookletSides::Both,
            booklet_sheets: 0,
            back_cover: false,
            poster_scale: 200.0,
            overlap_mm: 5.0,
            cut_marks: true,
            labels: true,
            orientation: Orientation::Auto,
            subset: Subset::All,
            reverse: false,
            as_image: false,
            image_dpi: 300,
            win_dpi: 300,
            printers: HashMap::new(),
            pdf_paper: String::new(),
        }
    }
}

impl PrintPrefs {
    pub fn load(st: AppState) -> Self {
        serde_json::from_str(&st.value(PREFS_KEY)).unwrap_or_default()
    }

    /// The layout of the plan.
    pub fn layout(&self) -> Layout {
        match self.mode {
            Mode::Size => Layout::Single {
                sizing: match self.size {
                    SizeChoice::Fit => Sizing::Fit,
                    SizeChoice::Actual => Sizing::Actual,
                    SizeChoice::Shrink => Sizing::ShrinkOversized,
                    SizeChoice::Custom => Sizing::Custom(self.scale),
                },
                auto_rotate: self.auto_rotate,
                centre: self.auto_rotate,
                paper_per_page: self.paper_by_page,
            },
            Mode::Multiple => Layout::NUp {
                per_sheet: self.per_sheet,
                order: self.order,
                gutter: self.gutter_mm.max(0.0) * MM,
                borders: self.borders,
                auto_rotate: self.auto_rotate,
            },
            Mode::Booklet => Layout::Booklet {
                binding: self.binding,
                sides: self.booklet_sides,
                sheets_per_booklet: self.booklet_sheets,
                back_cover: self.back_cover,
                gutter: 0.0,
            },
            Mode::Poster => Layout::Poster {
                scale_pct: self.poster_scale,
                overlap: self.overlap_mm.max(0.0) * MM,
                cut_marks: self.cut_marks,
                labels: self.labels,
            },
        }
    }

    /// The options of the plan on `paper`.
    pub fn options(&self, paper: Paper, alternatives: Vec<Paper>, sides: Sides) -> PrintOptions {
        PrintOptions {
            subset: self.subset,
            reverse: self.reverse,
            layout: self.layout(),
            orientation: if self.mode == Mode::Booklet {
                Orientation::Auto
            } else {
                self.orientation
            },
            paper,
            alternatives,
            sides,
        }
    }
}

/// The usual paper of the region of this computer.
fn locale_paper() -> &'static str {
    let locale = ["LC_PAPER", "LC_ALL", "LANG"]
        .iter()
        .find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()))
        .unwrap_or_default();
    print::default_paper_key(&locale)
}

fn standard_media() -> Vec<Media> {
    STANDARD_PAPERS
        .iter()
        .map(|p| Media {
            key: p.key.to_string(),
            name: p.name.to_string(),
            width: p.width,
            height: p.height,
            margins: [0.0; 4],
        })
        .collect()
}

/// The paper among `media` first named in `wanted`, else the first.
fn choose_paper(media: &[Media], wanted: &[&str]) -> String {
    wanted
        .iter()
        .find(|w| !w.is_empty() && media.iter().any(|m| m.key == **w))
        .map(|w| w.to_string())
        .or_else(|| media.first().map(|m| m.key.clone()))
        .unwrap_or_default()
}

fn media_label(lang: Lang, m: &Media) -> String {
    match m.name.as_str() {
        "Letter" => t(lang, "dialog.print.paper.letter"),
        "Legal" => t(lang, "dialog.print.paper.legal"),
        "Tabloid" => t(lang, "dialog.print.paper.tabloid"),
        "Executive" => t(lang, "dialog.print.paper.executive"),
        "" => tf(
            lang,
            "dialog.print.paper.size",
            &[
                ("w", &format!("{:.0}", m.width / MM)),
                ("h", &format!("{:.0}", m.height / MM)),
            ],
        ),
        name => name.to_string(),
    }
}

/// One document of the window.
#[derive(Clone)]
struct Doc {
    name: String,
    seed: Option<PrintSeed>,
    count: usize,
    geoms: Vec<PageGeom>,
    labels: Vec<String>,
    current: usize,
    selected: Vec<usize>,
    permission: PrintPermission,
    source: Option<Arc<PrintSource>>,
    failed: Option<String>,
    /// Comments hidden by the filter of the view (they do not print).
    filtered: usize,
}

fn tab_doc(t: &DocTab) -> Doc {
    let mut seed = t.doc.print_seed();
    seed.hidden = t.filtered_out();
    seed.layers = t.view_layer_states();
    Doc {
        name: t.doc.display_name(),
        filtered: seed.hidden.len(),
        seed: Some(seed),
        count: t.doc.page_count(),
        geoms: t.page_sizes.iter().map(|s| PageGeom::from_size(s, 1.0)).collect(),
        labels: t.doc.page_labels(),
        current: t.page,
        selected: t.selected_pages.clone(),
        permission: t.doc.print_permission(),
        source: None,
        failed: None,
    }
}

/// A document of a file not open in a tab.
fn file_doc(name: &str, bytes: &[u8]) -> Doc {
    match folitecta_core::Document::from_bytes(bytes.to_vec(), None, None) {
        Ok(mut doc) => {
            doc.set_name_hint(name);
            Doc {
                name: name.to_string(),
                seed: Some(doc.print_seed()),
                count: doc.page_count(),
                geoms: doc
                    .page_sizes()
                    .iter()
                    .map(|s| PageGeom::from_size(s, 1.0))
                    .collect(),
                labels: doc.page_labels(),
                current: 0,
                selected: Vec::new(),
                permission: doc.print_permission(),
                source: None,
                failed: None,
                filtered: 0,
            }
        }
        Err(e) => Doc {
            name: name.to_string(),
            seed: None,
            count: 0,
            geoms: Vec::new(),
            labels: Vec::new(),
            current: 0,
            selected: Vec::new(),
            permission: PrintPermission::Allowed,
            source: None,
            failed: Some(e.to_string()),
            filtered: 0,
        },
    }
}

/// Which pages of a single document print.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    All,
    Current,
    Visible,
    Selected,
    Range,
    /// The pages the command gave (bookmarks).
    Given,
}

/// The generation of the newest preview asked for.
static PREVIEW_GEN: AtomicU64 = AtomicU64::new(0);

/// The message of a plan error.
fn plan_error(lang: Lang, e: PlanError) -> String {
    t(
        lang,
        match e {
            PlanError::NoPages => "dialog.print.err.no_pages",
            PlanError::BadScale => "dialog.print.err.scale",
            PlanError::OverlapTooLarge => "dialog.print.err.overlap",
            PlanError::NoPrintableArea => "dialog.print.err.margins",
            PlanError::TooManySheets => "dialog.print.err.too_many",
        },
    )
}

fn range_error(lang: Lang, e: &RangeError) -> String {
    match e {
        RangeError::Bad(part) => tf(lang, "dialog.print.err.range", &[("part", part)]),
        RangeError::OutOfRange(n) => tf(lang, "dialog.print.err.range_count", &[("count", &n.to_string())]),
    }
}

fn state_text(lang: Lang, p: &PrinterInfo) -> String {
    let mut s = match p.state {
        PrinterState::Idle => t(lang, "dialog.print.state.idle"),
        PrinterState::Printing => t(lang, "dialog.print.state.printing"),
        PrinterState::Stopped => t(lang, "dialog.print.state.stopped"),
        PrinterState::Offline => t(lang, "dialog.print.state.offline"),
        PrinterState::Unknown => t(lang, "dialog.print.state.unknown"),
    };
    if !p.message.is_empty() {
        s = format!("{s}: {}", p.message);
    }
    if p.jobs > 0 {
        s = format!(
            "{s} · {}",
            tf(lang, "dialog.print.state.jobs", &[("count", &p.jobs.to_string())])
        );
    }
    s
}

/// The pages of the canvas in view (on mount).
const VISIBLE_JS: &str = r#"
const c = document.getElementById('canvas');
if (!c) return [];
const r = c.getBoundingClientRect();
return [...c.querySelectorAll('[id^="pg-"]')]
  .filter(e => { const b = e.getBoundingClientRect(); return b.bottom > r.top && b.top < r.bottom && b.right > r.left && b.left < r.right; })
  .map(e => parseInt(e.id.slice(3), 10)).filter(n => !isNaN(n));
"#;

#[component]
pub fn PrintWindow(target: PrintTarget) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let single = matches!(target, PrintTarget::Active | PrintTarget::Pages(_));
    let given = match &target {
        PrintTarget::Pages(p) => Some(p.clone()),
        _ => None,
    };
    let title = match &target {
        PrintTarget::Active | PrintTarget::Pages(_) => t(lang, "dialog.print.title"),
        PrintTarget::AllOpen => t(lang, "dialog.print.title_all"),
        PrintTarget::Files(f) => tf(
            lang,
            "dialog.print.title_docs",
            &[("count", &f.len().to_string())],
        ),
    };

    // The documents, then their print sources read away from the interface.
    let target_once = target.clone();
    let mut docs: Signal<Vec<Doc>> = use_signal(move || {
        let tabs = st.docs.peek();
        match &target_once {
            PrintTarget::Active | PrintTarget::Pages(_) => {
                let active = st.active_index();
                active
                    .and_then(|i| tabs.get(i))
                    .map(tab_doc)
                    .into_iter()
                    .collect()
            }
            PrintTarget::AllOpen => tabs.iter().map(tab_doc).collect(),
            PrintTarget::Files(files) => files
                .iter()
                .map(|(name, _)| Doc {
                    name: name.clone(),
                    seed: None,
                    count: 0,
                    geoms: Vec::new(),
                    labels: Vec::new(),
                    current: 0,
                    selected: Vec::new(),
                    permission: PrintPermission::Allowed,
                    source: None,
                    failed: None,
                    filtered: 0,
                })
                .collect(),
        }
    });
    let target_load = target.clone();
    use_hook(move || {
        spawn(async move {
            if let PrintTarget::Files(files) = target_load {
                for (i, (name, bytes)) in files.into_iter().enumerate() {
                    let made = tokio::task::spawn_blocking(move || file_doc(&name, &bytes)).await;
                    if let Ok(d) = made {
                        if let Some(slot) = docs.write().get_mut(i) {
                            *slot = d;
                        }
                    }
                }
            }
            let n = docs.peek().len();
            for i in 0..n {
                let Some(seed) = docs.peek().get(i).and_then(|d| d.seed.clone()) else {
                    continue;
                };
                let loaded = tokio::task::spawn_blocking(move || PrintSource::load(seed)).await;
                let mut list = docs.write();
                let Some(d) = list.get_mut(i) else { continue };
                match loaded {
                    Ok(Ok(src)) => {
                        d.geoms = src.geoms().to_vec();
                        d.labels = src.labels().to_vec();
                        d.count = src.page_count();
                        d.source = Some(Arc::new(src));
                    }
                    Ok(Err(e)) => d.failed = Some(e.to_string()),
                    Err(e) => d.failed = Some(e.to_string()),
                }
            }
        });
    });

    let mut prefs = use_signal(|| PrintPrefs::load(st));
    let mut copies = use_signal(|| "1".to_string());
    let has_selection = docs.peek().first().is_some_and(|d| !d.selected.is_empty());
    let mut scope = use_signal(move || {
        if given.is_some() {
            Scope::Given
        } else if has_selection {
            Scope::Selected
        } else {
            Scope::All
        }
    });
    let mut range = use_signal(String::new);
    let mut use_labels = use_signal(|| false);
    let mut visible: Signal<Vec<usize>> = use_signal(Vec::new);
    use_hook(move || {
        spawn(async move {
            if let Ok(v) = document::eval(VISIBLE_JS).await {
                if let Ok(list) = serde_json::from_value::<Vec<usize>>(v) {
                    visible.set(list);
                }
            }
        });
    });

    // Printers and their capabilities.
    let mut printers: Signal<Option<Result<Vec<PrinterInfo>, PrintError>>> = use_signal(|| None);
    let mut caps: Signal<HashMap<String, Result<PrinterCaps, PrintError>>> = use_signal(HashMap::new);
    let mut destination = use_signal(|| PrintPrefs::load(st).destination);
    use_hook(move || {
        spawn(async move {
            let list = tokio::task::spawn_blocking(|| printing::backend().printers())
                .await
                .unwrap_or_else(|e| Err(PrintError::Io(e.to_string())));
            let names: Vec<String> = list
                .as_ref()
                .map(|l| l.iter().map(|p| p.name.clone()).collect())
                .unwrap_or_default();
            let default = list
                .as_ref()
                .ok()
                .and_then(|l| l.iter().find(|p| p.is_default).or(l.first()))
                .map(|p| p.name.clone());
            let current = destination.peek().clone();
            if !(current == PDF || current == SYSTEM || names.contains(&current)) {
                destination.set(default.unwrap_or_else(|| SYSTEM.to_string()));
            }
            printers.set(Some(list));
        });
    });
    let is_printer = move || {
        let d = destination();
        !d.is_empty() && d != PDF && d != SYSTEM
    };
    // Capabilities of the printer chosen, once for each printer.
    use_effect(move || {
        let d = destination();
        if d.is_empty() || d == PDF || d == SYSTEM || caps.peek().contains_key(&d) {
            return;
        }
        spawn(async move {
            let name = d.clone();
            let c = tokio::task::spawn_blocking(move || printing::backend().capabilities(&name))
                .await
                .unwrap_or_else(|e| Err(PrintError::Io(e.to_string())));
            caps.write().insert(d, c);
        });
    });
    let current_caps = move || -> Option<PrinterCaps> {
        let d = destination();
        caps.read().get(&d).and_then(|c| c.as_ref().ok().cloned())
    };
    let media = use_memo(move || -> Vec<Media> {
        match current_caps() {
            Some(c) if !c.media.is_empty() => c.media,
            _ => standard_media(),
        }
    });
    let mut paper_key = use_signal(String::new);
    let mut sides = use_signal(|| Sides::OneSided);
    let mut gray = use_signal(|| false);
    // The paper, sides and colour of the destination chosen (remembered for each printer).
    use_effect(move || {
        let d = destination();
        let list = media();
        let c = current_caps();
        let p = prefs.peek().clone();
        let remembered = if d == PDF || d == SYSTEM {
            PrinterPrefs {
                paper: p.pdf_paper.clone(),
                ..Default::default()
            }
        } else {
            p.printers.get(&d).cloned().unwrap_or_default()
        };
        let default_media = c
            .as_ref()
            .and_then(|c| c.default_media.clone())
            .unwrap_or_default();
        let key = choose_paper(&list, &[&remembered.paper, &default_media, locale_paper()]);
        if *paper_key.peek() != key {
            paper_key.set(key);
        }
        let allowed = c
            .as_ref()
            .map(|c| c.sides.clone())
            .unwrap_or_else(|| vec![Sides::OneSided, Sides::LongEdge, Sides::ShortEdge]);
        let s = remembered
            .sides
            .or(c.as_ref().map(|c| c.default_sides))
            .filter(|s| allowed.contains(s))
            .unwrap_or(Sides::OneSided);
        sides.set(s);
        gray.set(
            remembered
                .gray
                .unwrap_or_else(|| c.as_ref().is_some_and(|c| c.color && !c.default_color)),
        );
    });
    let paper = move || -> Paper {
        let key = paper_key();
        media()
            .iter()
            .find(|m| m.key == key)
            .map(Media::paper)
            .unwrap_or_default()
    };
    let alternatives = move || -> Vec<Paper> { media().iter().map(Media::paper).collect() };

    // The pages of each document, then the plans.
    let target_pages = target.clone();
    let pages_of = move |d: &Doc| -> Result<Vec<usize>, String> {
        if !single {
            return Ok((0..d.count).collect());
        }
        let n = d.count;
        Ok(match scope() {
            Scope::All => (0..n).collect(),
            Scope::Current => vec![d.current.min(n.saturating_sub(1))],
            Scope::Visible => {
                let v = visible();
                if v.is_empty() { vec![d.current] } else { v }
            }
            Scope::Selected => {
                let mut s = d.selected.clone();
                s.sort_unstable();
                s
            }
            Scope::Given => match &target_pages {
                PrintTarget::Pages(p) => p.clone(),
                _ => (0..n).collect(),
            },
            Scope::Range => {
                parse_print_range(&range(), n, &d.labels, use_labels()).map_err(|e| range_error(lang, &e))?
            }
        })
    };
    let plans = use_memo(move || -> Result<Vec<(usize, PrintPlan)>, String> {
        let p = prefs();
        let paper = paper();
        let alternatives = alternatives();
        let opts = p.options(paper, alternatives, sides());
        let list = docs.read();
        let mut out = Vec::new();
        for (i, d) in list.iter().enumerate() {
            if d.failed.is_some() || d.count == 0 {
                continue;
            }
            let pages = pages_of(d)?;
            let plan = print::plan(&pages, &d.geoms, &opts).map_err(|e| plan_error(lang, e))?;
            out.push((i, plan));
        }
        if out.is_empty() {
            return Err(plan_error(lang, PlanError::NoPages));
        }
        Ok(out)
    });
    let total_sides = move || -> usize {
        plans
            .read()
            .as_ref()
            .map(|p| p.iter().map(|(_, plan)| plan.sheets.len()).sum())
            .unwrap_or(0)
    };

    // The preview of one sheet.
    let mut index = use_signal(|| 0usize);
    let mut shown: Signal<Option<u64>> = use_signal(|| None);
    let mut preview_error: Signal<Option<String>> = use_signal(|| None);
    use_effect(move || {
        let total = total_sides();
        if index() >= total.max(1) {
            index.set(total.saturating_sub(1));
            return;
        }
        let i = index();
        let content = prefs.read().content;
        let Ok(list) = plans.read().clone() else {
            return;
        };
        // The document and the sheet shown.
        let mut k = i;
        let mut found = None;
        for (d, plan) in list {
            if k < plan.sheets.len() {
                found = Some((d, plan, k));
                break;
            }
            k -= plan.sheets.len();
        }
        let Some((d, plan, sheet)) = found else { return };
        let Some(src) = docs.read().get(d).and_then(|x| x.source.clone()) else {
            return;
        };
        let generation = PREVIEW_GEN.fetch_add(1, Ordering::Relaxed) + 1;
        spawn(async move {
            tokio::time::sleep(Duration::from_millis(120)).await;
            if PREVIEW_GEN.load(Ordering::Relaxed) != generation {
                return;
            }
            let made = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, String> {
                let o = BuildOptions {
                    content,
                    compress: false,
                };
                let bytes = src
                    .build(&plan, sheet..sheet + 1, &o, &mut |_, _| {
                        PREVIEW_GEN.load(Ordering::Relaxed) == generation
                    })
                    .map_err(|e| e.to_string())?
                    .ok_or_else(String::new)?;
                let s = &plan.sheets[sheet];
                let scale = 900.0 / s.paper.width.max(s.paper.height).max(1.0);
                let r = folitecta_core::Renderer::new(Arc::new(bytes)).map_err(|e| e.to_string())?;
                let opts = folitecta_core::RenderOpts {
                    annotations: false,
                    transparent: false,
                };
                let img = r.render_rgba(0, scale, opts).map_err(|e| e.to_string())?;
                // Shown as it is read: a sheet laid out across the paper turned upright.
                let img = if s.landscape {
                    image::imageops::rotate90(&img)
                } else {
                    img
                };
                let mut png = Vec::new();
                image::DynamicImage::ImageRgba8(img)
                    .to_rgb8()
                    .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
                    .map_err(|e| e.to_string())?;
                Ok(png)
            })
            .await;
            if PREVIEW_GEN.load(Ordering::Relaxed) != generation {
                return;
            }
            match made {
                Ok(Ok(png)) => {
                    runner::store_preview(generation, png);
                    preview_error.set(None);
                    shown.set(Some(generation));
                }
                Ok(Err(e)) if !e.is_empty() => preview_error.set(Some(e)),
                Ok(Err(_)) => {}
                Err(e) => preview_error.set(Some(e.to_string())),
            }
        });
    });

    // What the window shows.
    let job_running = JOB.read().running;
    let docs_now = docs.read().clone();
    let doc0 = docs_now.first().cloned();
    let loaded = docs_now.iter().all(|d| d.source.is_some() || d.failed.is_some());
    let forbidden = docs_now
        .iter()
        .any(|d| d.permission == PrintPermission::Forbidden);
    let low = docs_now
        .iter()
        .any(|d| d.permission == PrintPermission::LowQuality);
    let filtered = docs_now.iter().map(|d| d.filtered).sum::<usize>();
    let p = prefs();
    let printers_now = printers();
    let caps_now = current_caps();
    let dest = destination();
    let printer_info = printers_now
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .and_then(|l| l.iter().find(|x| x.name == dest).cloned());
    let plans_now = plans();
    let total = total_sides();
    let i = index().min(total.saturating_sub(1));
    // The sheet shown, its document and its plan.
    let (sheet_now, plan_now) = match &plans_now {
        Ok(list) => {
            let mut k = i;
            let mut found = (None, None);
            for (_, plan) in list {
                if k < plan.sheets.len() {
                    found = (Some(plan.sheets[k].clone()), Some(plan.clone()));
                    break;
                }
                k -= plan.sheets.len();
            }
            found
        }
        Err(_) => (None, None),
    };
    let duplex_plan = plan_now.as_ref().is_some_and(|p| p.sides.duplex());
    let no_duplex = is_printer() && caps_now.as_ref().is_some_and(|c| !c.duplex) && duplex_plan;
    let pages_total: usize = plans_now
        .as_ref()
        .map(|l| l.iter().map(|(_, p)| p.pages.len()).sum())
        .unwrap_or(0);
    let physical: usize = plans_now
        .as_ref()
        .map(|l| l.iter().map(|(_, p)| p.physical_sheets).sum())
        .unwrap_or(0);
    let copies_n: u32 = copies().trim().parse::<u32>().unwrap_or(1).clamp(1, 999);
    let max_copies = caps_now.as_ref().map(|c| c.max_copies).unwrap_or(999).max(1);
    let printer_ready = !is_printer() || caps.read().get(&dest).is_some_and(|c| c.is_ok());
    let blocked = forbidden
        || plans_now.is_err()
        || job_running
        || !loaded
        || no_duplex
        || !printer_ready
        || dest.is_empty();
    let ok_label = match dest.as_str() {
        PDF => t(lang, "dialog.print.save"),
        SYSTEM => t(lang, "dialog.print.open_system"),
        _ => t(lang, "dialog.print.go"),
    };

    let mut warnings: Vec<String> = Vec::new();
    if forbidden {
        warnings.push(t(lang, "dialog.print.warn.forbidden"));
    } else if low {
        warnings.push(t(lang, "dialog.print.warn.low"));
    }
    if job_running {
        warnings.push(t(lang, "dialog.print.warn.busy"));
    }
    if no_duplex {
        warnings.push(t(lang, "dialog.print.warn.no_duplex"));
    }
    if let Ok(list) = &plans_now {
        for (_, plan) in list {
            for w in &plan.warnings {
                match w {
                    PlanWarning::Cut(pages) => warnings.push(tf(
                        lang,
                        "dialog.print.warn.cut",
                        &[("pages", &format_pages(pages))],
                    )),
                    PlanWarning::Blanks(n) => {
                        warnings.push(tf(lang, "dialog.print.warn.blank", &[("count", &n.to_string())]))
                    }
                    PlanWarning::Tiles { .. } => {}
                }
            }
        }
    }
    if filtered > 0 {
        warnings.push(tf(
            lang,
            "dialog.print.warn.filtered",
            &[("count", &filtered.to_string())],
        ));
    }
    if dest == SYSTEM {
        warnings.push(t(lang, "dialog.print.warn.system"));
    }
    if is_printer() && p.mode == Mode::Size && p.paper_by_page && !cfg!(target_os = "windows") {
        warnings.push(t(lang, "dialog.print.warn.paper_by_page"));
    }
    if cfg!(target_os = "windows") && is_printer() {
        let paper_now = paper();
        let dpi = if p.win_dpi == 0 { 600 } else { p.win_dpi };
        let bytes = printing::spool_estimate(
            paper_now.width,
            paper_now.height,
            dpi,
            gray(),
            total * copies_n as usize,
        );
        warnings.push(tf(
            lang,
            "dialog.print.warn.windows_raster",
            &[("size", &crate::util::human_size(bytes))],
        ));
    }
    let tiles = plans_now.as_ref().ok().and_then(|l| {
        l.iter()
            .flat_map(|(_, p)| p.warnings.iter())
            .find_map(|w| match w {
                PlanWarning::Tiles { cols, rows } => Some((*cols, *rows)),
                _ => None,
            })
    });

    // The preview stage: the sheet as it is read (a sheet laid out across
    // the paper is shown turned), its proportions and its printable area.
    let (pw, ph, margins) = sheet_now
        .as_ref()
        .map(|s| {
            let [l, b, r, t] = s.paper.margins;
            if s.landscape {
                (s.paper.height, s.paper.width, [b, r, t, l])
            } else {
                (s.paper.width, s.paper.height, [l, b, r, t])
            }
        })
        .unwrap_or_else(|| {
            let p = paper();
            (p.width, p.height, p.margins)
        });
    let area_style = format!(
        "left:{:.3}%;bottom:{:.3}%;right:{:.3}%;top:{:.3}%",
        margins[0] / pw * 100.0,
        margins[1] / ph * 100.0,
        margins[2] / pw * 100.0,
        margins[3] / ph * 100.0
    );
    let show_area = margins.iter().any(|m| *m > 0.0);
    let ratio = pw / ph.max(1.0);
    let stage_style = format!(
        "width: min(100cqw, calc(100cqh * {ratio:.5})); height: min(100cqh, calc(100cqw / {ratio:.5}))"
    );
    let face_text = match sheet_now.as_ref().map(|s| (s.face, s.physical)) {
        Some((Face::Front, n)) => format!(
            " ({})",
            tf(lang, "dialog.print.sheet_front", &[("n", &(n + 1).to_string())])
        ),
        Some((Face::Back, n)) => format!(
            " ({})",
            tf(lang, "dialog.print.sheet_back", &[("n", &(n + 1).to_string())])
        ),
        _ => String::new(),
    };
    let side_label = format!(
        "{}{face_text}",
        tf(
            lang,
            "dialog.print.side_label",
            &[
                ("n", &(i + 1).min(total).to_string()),
                ("count", &total.to_string())
            ]
        )
    );
    let summary = {
        let base = tf(
            lang,
            "dialog.print.summary",
            &[
                ("pages", &pages_total.to_string()),
                ("sides", &total.to_string()),
                ("sheets", &physical.to_string()),
            ],
        );
        if copies_n > 1 && dest != PDF {
            format!(
                "{base} {}",
                tf(
                    lang,
                    "dialog.print.summary_copies",
                    &[("copies", &copies_n.to_string())]
                )
            )
        } else {
            base
        }
    };
    let shown_gen = shown();

    // Pressing Print, Save or System dialog.
    let target_go = target.clone();
    let go = move |_| {
        if blocked {
            return;
        }
        let Ok(list) = plans.peek().clone() else { return };
        let d = destination.peek().clone();
        // Remember the settings (copies are not).
        let mut keep = prefs.peek().clone();
        keep.destination = d.clone();
        if d == PDF || d == SYSTEM {
            keep.pdf_paper = paper_key.peek().clone();
        } else {
            keep.printers.insert(
                d.clone(),
                PrinterPrefs {
                    paper: paper_key.peek().clone(),
                    sides: Some(*sides.peek()),
                    gray: Some(*gray.peek()),
                },
            );
        }
        st.set_value(PREFS_KEY, serde_json::to_string(&keep).unwrap_or_default());
        let all = docs.peek().clone();
        let items: Vec<PrintItem> = list
            .into_iter()
            .filter_map(|(i, plan)| {
                let doc = all.get(i)?;
                Some(PrintItem {
                    source: doc.source.clone()?,
                    plan,
                    name: doc.name.clone(),
                })
            })
            .collect();
        if items.is_empty() {
            return;
        }
        let media_now = media.peek().iter().find(|m| m.key == *paper_key.peek()).cloned();
        let by_page = keep.mode == Mode::Size && keep.paper_by_page;
        let plan_sides = items[0].plan.sides;
        #[allow(unused_mut)]
        let mut ticket = JobTicket {
            printer: d.clone(),
            job_name: items[0].name.clone(),
            copies: if d == PDF {
                1
            } else {
                copies_n.min(max_copies.max(999))
            },
            collate: keep.collate,
            sides: plan_sides,
            media: if by_page { None } else { media_now },
            gray: *gray.peek(),
            dpi: keep.win_dpi,
            devmode: None,
        };
        #[cfg(target_os = "windows")]
        {
            let chosen: Option<(String, Vec<u8>)> = (*PROPERTIES.peek()).clone();
            ticket.devmode = chosen.filter(|(p, _)| *p == d).map(|(_, b)| b);
        }
        let as_image = (keep.as_image && !cfg!(target_os = "windows")).then_some(keep.image_dpi);
        let content = keep.content;
        let multi = items.len() > 1 || !matches!(target_go, PrintTarget::Active | PrintTarget::Pages(_));
        st.set_dialog(Dialog::None);
        if d == PDF {
            let suggested = runner::save_name(&items[0].name);
            dioxus::core::spawn_forever(async move {
                let path = if multi && items.len() > 1 {
                    rfd::AsyncFileDialog::new()
                        .pick_folder()
                        .await
                        .map(|f| f.path().to_path_buf())
                } else {
                    crate::files::pdf_dialog()
                        .set_file_name(&suggested)
                        .save_file()
                        .await
                        .map(|f| f.path().to_path_buf())
                };
                let Some(path) = path else { return };
                runner::start(
                    st,
                    PrintRequest {
                        items,
                        content,
                        ticket,
                        dest: Destination::SavePdf(path),
                        as_image: None,
                    },
                );
            });
            return;
        }
        let dest = if d == SYSTEM {
            Destination::System
        } else {
            Destination::Printer(d)
        };
        runner::start(
            st,
            PrintRequest {
                items,
                content,
                ticket,
                dest,
                as_image,
            },
        );
    };

    let mut set_prefs = move |f: &dyn Fn(&mut PrintPrefs)| {
        f(&mut prefs.write());
    };
    let printer_list: Vec<PrinterInfo> = printers_now
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .map(|l| {
            let mut l = l.clone();
            l.sort_by(|a, b| {
                b.is_default
                    .cmp(&a.is_default)
                    .then(a.label.to_lowercase().cmp(&b.label.to_lowercase()))
            });
            l
        })
        .unwrap_or_default();
    let server_hint = match &printers_now {
        None => Some(("searching", t(lang, "dialog.print.searching"))),
        Some(Err(PrintError::NoServer)) => Some(("warn", t(lang, "dialog.print.no_server"))),
        Some(Err(PrintError::RemoteServer(_))) => Some(("warn", t(lang, "dialog.print.remote_server"))),
        Some(Err(e)) => Some((
            "warn",
            tf(lang, "toast.print_failed", &[("reason", &e.to_string())]),
        )),
        Some(Ok(l)) if l.is_empty() => Some(("warn", t(lang, "dialog.print.no_printers"))),
        _ => None,
    };
    let caps_error = if is_printer() {
        match caps.read().get(&dest) {
            Some(Err(e)) => Some(tf(lang, "toast.print_failed", &[("reason", &e.to_string())])),
            _ => None,
        }
    } else {
        None
    };
    let media_now = media();
    let key_now = paper_key();
    let sides_allowed = caps_now
        .as_ref()
        .map(|c| c.sides.clone())
        .unwrap_or_else(|| vec![Sides::OneSided, Sides::LongEdge, Sides::ShortEdge]);
    let sides_fixed = matches!(p.mode, Mode::Booklet | Mode::Poster);
    let shows_color = caps_now.as_ref().is_none_or(|c| c.color) && dest != PDF;
    let doc_labels = doc0.as_ref().map(|d| d.labels.clone()).unwrap_or_default();
    let has_labels = doc_labels
        .iter()
        .enumerate()
        .any(|(i, l)| *l != (i + 1).to_string());
    let page_count = doc0.as_ref().map(|d| d.count).unwrap_or(0);
    let selected_text = doc0
        .as_ref()
        .map(|d| format_pages(&d.selected))
        .unwrap_or_default();
    let visible_text = format_pages(&visible());
    let docs_summary = tf(
        lang,
        "dialog.print.docs_summary",
        &[
            ("docs", &docs_now.len().to_string()),
            (
                "pages",
                &docs_now.iter().map(|d| d.count).sum::<usize>().to_string(),
            ),
        ],
    );
    let failed_docs: Vec<String> = docs_now
        .iter()
        .filter_map(|d| d.failed.as_ref().map(|e| format!("{}: {e}", d.name)))
        .collect();
    let is_windows = cfg!(target_os = "windows");
    let modes: Vec<(Mode, &'static str, &'static str, String)> = [
        (Mode::Size, "page", "size"),
        (Mode::Multiple, "grid", "multiple"),
        (Mode::Booklet, "book", "booklet"),
        (Mode::Poster, "arrows-maximize", "poster"),
    ]
    .into_iter()
    .map(|(m, icon, key)| (m, icon, key, t(lang, &format!("dialog.print.mode.{key}"))))
    .collect();
    let size_choices: Vec<(SizeChoice, String)> = [
        (SizeChoice::Fit, "fit"),
        (SizeChoice::Actual, "actual"),
        (SizeChoice::Shrink, "shrink"),
        (SizeChoice::Custom, "custom"),
    ]
    .into_iter()
    .map(|(c, key)| (c, t(lang, &format!("dialog.print.size.{key}"))))
    .collect();
    let orders: Vec<(NupOrder, &'static str, String)> = [
        (NupOrder::Horizontal, "horizontal"),
        (NupOrder::HorizontalReversed, "horizontal_rev"),
        (NupOrder::Vertical, "vertical"),
        (NupOrder::VerticalReversed, "vertical_rev"),
    ]
    .into_iter()
    .map(|(o, key)| (o, key, t(lang, &format!("dialog.print.nup.order.{key}"))))
    .collect();
    let shown_sides = plan_now.as_ref().map(|p| p.sides).unwrap_or(sides());
    let side_choices: Vec<(Sides, &'static str, String)> = [
        (Sides::OneSided, "one"),
        (Sides::LongEdge, "long"),
        (Sides::ShortEdge, "short"),
    ]
    .into_iter()
    .filter(|(s, _)| sides_allowed.contains(s) || *s == shown_sides)
    .map(|(s, key)| (s, key, t(lang, &format!("dialog.print.sides.{key}"))))
    .collect();
    let mut step = move |delta: i64| {
        let n = total_sides() as i64;
        let next = (index() as i64 + delta).clamp(0, (n - 1).max(0));
        index.set(next as usize);
    };

    rsx! {
        Frame { title, xl: true, ok_label: Some(ok_label), ok_disabled: blocked, on_ok: go,
            div { class: "print-window",
                onkeydown: move |e| match e.key() {
                    Key::PageDown => { e.prevent_default(); step(1) }
                    Key::PageUp => { e.prevent_default(); step(-1) }
                    _ => {}
                },
                div { class: "print-settings",
                    // The printer.
                    div { class: "field",
                        label { r#for: "print-printer", "{t(lang, \"dialog.print.printer\")}" }
                        select { id: "print-printer", class: "print-printer", title: "{t(lang, \"dialog.print.printer\")}",
                            onchange: move |e| destination.set(e.value()),
                            for pr in printer_list.iter() {
                                option { key: "{pr.name}", value: "{pr.name}", selected: dest == pr.name,
                                    if pr.is_default { "{tf(lang, \"dialog.print.default_printer\", &[(\"name\", &pr.label)])}" } else { "{pr.label}" }
                                }
                            }
                            if !printer_list.is_empty() {
                                option { disabled: true, value: "", "──────────" }
                            }
                            option { value: PDF, selected: dest == PDF, "{t(lang, \"dialog.print.save_pdf\")}" }
                            option { value: SYSTEM, selected: dest == SYSTEM, "{t(lang, \"dialog.print.system_dialog\")}" }
                        }
                        if let Some(info) = printer_info.as_ref() {
                            div { class: "hint print-state", "{state_text(lang, info)}" }
                        }
                        if let Some((kind, text)) = server_hint.clone() {
                            div { class: if kind == "warn" { "hint print-hint warn" } else { "hint print-hint" },
                                if kind == "warn" { Icon { name: "printer-off" } }
                                span { "{text}" }
                            }
                        }
                        if let Some(e) = caps_error.clone() {
                            div { class: "hint print-hint warn", "{e}" }
                        }
                        if is_windows && is_printer() {
                            PropertiesButton { printer: dest.clone(), sides, gray, copies, paper_key }
                        }
                    }
                    if dest != PDF {
                        div { class: "field-grid print-copies",
                            div { class: "field",
                                label { r#for: "print-copies", "{t(lang, \"dialog.print.copies\")}" }
                                input { id: "print-copies", r#type: "number", min: "1", max: "{max_copies.max(1)}", value: "{copies}",
                                    title: "{t(lang, \"dialog.print.copies\")}",
                                    oninput: move |e| copies.set(e.value()) }
                            }
                            label { class: "check",
                                input { r#type: "checkbox", checked: p.collate, disabled: copies_n < 2,
                                    onchange: move |e| set_prefs(&|p| p.collate = e.checked()) }
                                span { "{t(lang, \"dialog.print.collate\")}" }
                            }
                        }
                    }
                    // The pages.
                    if single {
                        div { class: "field print-pages",
                            label { "{t(lang, \"dialog.pages\")}" }
                            div { class: "radio-col",
                                label { input { r#type: "radio", name: "print-scope", checked: scope() == Scope::All, onchange: move |_| scope.set(Scope::All) }
                                    span { "{t(lang, \"dialog.pages.all\")} ({page_count})" } }
                                label { input { r#type: "radio", name: "print-scope", checked: scope() == Scope::Current, onchange: move |_| scope.set(Scope::Current) }
                                    span { "{t(lang, \"dialog.pages.current\")}" } }
                                label { input { r#type: "radio", name: "print-scope", checked: scope() == Scope::Visible, onchange: move |_| scope.set(Scope::Visible) }
                                    span { "{t(lang, \"dialog.print.pages.visible\")} ({visible_text})" } }
                                if !selected_text.is_empty() {
                                    label { input { r#type: "radio", name: "print-scope", checked: scope() == Scope::Selected, onchange: move |_| scope.set(Scope::Selected) }
                                        span { "{t(lang, \"dialog.pages.selected\")} ({selected_text})" } }
                                }
                                if let PrintTarget::Pages(given) = &target {
                                    label { input { r#type: "radio", name: "print-scope", checked: scope() == Scope::Given, onchange: move |_| scope.set(Scope::Given) }
                                        span { "{t(lang, \"dialog.print.pages.given\")} ({format_pages(given)})" } }
                                }
                                label { input { r#type: "radio", name: "print-scope", class: "print-range-radio", checked: scope() == Scope::Range, onchange: move |_| scope.set(Scope::Range) }
                                    span { "{t(lang, \"dialog.print.pages.range\")}" } }
                            }
                            if scope() == Scope::Range {
                                input { class: "print-range", value: "{range}", placeholder: "1-3, 7, 10-", title: "{t(lang, \"dialog.print.pages.range\")}",
                                    oninput: move |e| range.set(e.value()) }
                                if has_labels {
                                    label { class: "check",
                                        input { r#type: "checkbox", checked: use_labels(), onchange: move |e| use_labels.set(e.checked()) }
                                        span { "{t(lang, \"dialog.print.use_labels\")}" }
                                    }
                                }
                            }
                        }
                    } else {
                        div { class: "hint print-docs", "{docs_summary}" }
                    }
                    for f in failed_docs.iter() {
                        div { class: "hint print-hint warn", "{f}" }
                    }
                    if p.mode != Mode::Booklet {
                        div { class: "print-subset-row",
                            div { class: "field",
                                label { r#for: "print-subset", "{t(lang, \"dialog.print.subset\")}" }
                                select { id: "print-subset", onchange: move |e| { let v = e.value(); set_prefs(&|p| p.subset = match v.as_str() { "odd" => Subset::Odd, "even" => Subset::Even, _ => Subset::All }) },
                                    option { value: "all", selected: p.subset == Subset::All, "{t(lang, \"dialog.print.subset.all\")}" }
                                    option { value: "odd", selected: p.subset == Subset::Odd, "{t(lang, \"dialog.print.subset.odd\")}" }
                                    option { value: "even", selected: p.subset == Subset::Even, "{t(lang, \"dialog.print.subset.even\")}" }
                                }
                            }
                            label { class: "check",
                                input { r#type: "checkbox", checked: p.reverse, onchange: move |e| set_prefs(&|p| p.reverse = e.checked()) }
                                span { "{t(lang, \"dialog.print.reverse\")}" }
                            }
                        }
                    }
                    // Comments and form fields.
                    div { class: "field",
                        label { r#for: "print-content", "{t(lang, \"dialog.print.content\")}" }
                        select { id: "print-content", onchange: move |e| { let v = e.value(); set_prefs(&|p| p.content = match v.as_str() { "document" => Content::Document, "fields" => Content::FormFieldsOnly, _ => Content::DocumentAndComments }) },
                            option { value: "document", selected: p.content == Content::Document, "{t(lang, \"dialog.print.content.document\")}" }
                            option { value: "comments", selected: p.content == Content::DocumentAndComments, "{t(lang, \"dialog.print.content.comments\")}" }
                            option { value: "fields", selected: p.content == Content::FormFieldsOnly, "{t(lang, \"dialog.print.content.fields\")}" }
                        }
                    }
                    // Page sizing and handling.
                    div { class: "field",
                        label { "{t(lang, \"dialog.print.mode\")}" }
                        div { class: "print-modes", role: "group",
                            for (m, icon, key, label) in modes.into_iter() {
                                button { key: "{key}", class: if p.mode == m { "tbtn active print-mode-{key}" } else { "tbtn print-mode-{key}" },
                                    title: "{label}",
                                    onclick: move |_| set_prefs(&|p| p.mode = m),
                                    Icon { name: icon }
                                    span { "{label}" }
                                }
                            }
                        }
                    }
                    {match p.mode {
                        Mode::Size => rsx! {
                            div { class: "radio-col print-size",
                                for (c, label) in size_choices.into_iter() {
                                    label { key: "{label}",
                                        input { r#type: "radio", name: "print-size", checked: p.size == c, onchange: move |_| set_prefs(&|p| p.size = c) }
                                        span { "{label}" }
                                    }
                                }
                            }
                            if p.size == SizeChoice::Custom {
                                div { class: "field",
                                    input { r#type: "number", min: "10", max: "1000", step: "5", value: "{p.scale}", title: "{t(lang, \"dialog.print.size.custom\")}",
                                        oninput: move |e| { let v = e.value().replace(',', ".").parse::<f32>().unwrap_or(100.0); set_prefs(&|p| p.scale = v) } }
                                }
                            }
                            label { class: "check",
                                input { r#type: "checkbox", checked: p.paper_by_page, onchange: move |e| set_prefs(&|p| p.paper_by_page = e.checked()) }
                                span { "{t(lang, \"dialog.print.paper_by_page\")}" }
                            }
                            label { class: "check",
                                input { r#type: "checkbox", checked: p.auto_rotate, onchange: move |e| set_prefs(&|p| p.auto_rotate = e.checked()) }
                                span { "{t(lang, \"dialog.print.auto_rotate\")}" }
                            }
                        },
                        Mode::Multiple => rsx! {
                            div { class: "field-grid",
                                div { class: "field",
                                    label { r#for: "print-per-sheet", "{t(lang, \"dialog.print.nup.per_sheet\")}" }
                                    select { id: "print-per-sheet", class: "print-per-sheet", onchange: move |e| { let v = e.value().parse::<u8>().unwrap_or(2); set_prefs(&|p| p.per_sheet = v) },
                                        for n in [2u8, 4, 6, 9, 16] {
                                            option { key: "{n}", value: "{n}", selected: p.per_sheet == n, "{n}" }
                                        }
                                    }
                                }
                                div { class: "field",
                                    label { r#for: "print-order", "{t(lang, \"dialog.print.nup.order\")}" }
                                    select { id: "print-order", onchange: move |e| { let v = e.value(); set_prefs(&|p| p.order = match v.as_str() { "horizontal_rev" => NupOrder::HorizontalReversed, "vertical" => NupOrder::Vertical, "vertical_rev" => NupOrder::VerticalReversed, _ => NupOrder::Horizontal }) },
                                        for (o, key, label) in orders.into_iter() {
                                            option { key: "{key}", value: "{key}", selected: p.order == o, "{label}" }
                                        }
                                    }
                                }
                                div { class: "field",
                                    label { r#for: "print-gutter", "{t(lang, \"dialog.print.nup.gutter\")}" }
                                    input { id: "print-gutter", r#type: "number", min: "0", max: "50", step: "1", value: "{p.gutter_mm}",
                                        oninput: move |e| { let v = e.value().replace(',', ".").parse::<f32>().unwrap_or(0.0).clamp(0.0, 50.0); set_prefs(&|p| p.gutter_mm = v) } }
                                }
                            }
                            label { class: "check",
                                input { r#type: "checkbox", checked: p.borders, onchange: move |e| set_prefs(&|p| p.borders = e.checked()) }
                                span { "{t(lang, \"dialog.print.nup.borders\")}" }
                            }
                            label { class: "check",
                                input { r#type: "checkbox", checked: p.auto_rotate, onchange: move |e| set_prefs(&|p| p.auto_rotate = e.checked()) }
                                span { "{t(lang, \"dialog.print.auto_rotate\")}" }
                            }
                        },
                        Mode::Booklet => rsx! {
                            div { class: "field-grid",
                                div { class: "field",
                                    label { r#for: "print-booklet-sides", "{t(lang, \"dialog.print.booklet.subset\")}" }
                                    select { id: "print-booklet-sides", onchange: move |e| { let v = e.value(); set_prefs(&|p| p.booklet_sides = match v.as_str() { "front" => BookletSides::FrontOnly, "back" => BookletSides::BackOnly, _ => BookletSides::Both }) },
                                        option { value: "both", selected: p.booklet_sides == BookletSides::Both, "{t(lang, \"dialog.print.booklet.both\")}" }
                                        option { value: "front", selected: p.booklet_sides == BookletSides::FrontOnly, "{t(lang, \"dialog.print.booklet.front\")}" }
                                        option { value: "back", selected: p.booklet_sides == BookletSides::BackOnly, "{t(lang, \"dialog.print.booklet.back\")}" }
                                    }
                                }
                                div { class: "field",
                                    label { r#for: "print-binding", "{t(lang, \"dialog.print.booklet.binding\")}" }
                                    select { id: "print-binding", onchange: move |e| { let v = e.value(); set_prefs(&|p| p.binding = if v == "right" { Binding::Right } else { Binding::Left }) },
                                        option { value: "left", selected: p.binding == Binding::Left, "{t(lang, \"dialog.print.booklet.left\")}" }
                                        option { value: "right", selected: p.binding == Binding::Right, "{t(lang, \"dialog.print.booklet.right\")}" }
                                    }
                                }
                                div { class: "field",
                                    label { r#for: "print-booklet-sheets", "{t(lang, \"dialog.print.booklet.sheets\")}" }
                                    select { id: "print-booklet-sheets", onchange: move |e| { let v = e.value().parse::<u16>().unwrap_or(0); set_prefs(&|p| p.booklet_sheets = v) },
                                        option { value: "0", selected: p.booklet_sheets == 0, "{t(lang, \"dialog.print.booklet.sheets_all\")}" }
                                        for n in 1u16..=50 {
                                            option { key: "{n}", value: "{n}", selected: p.booklet_sheets == n, "{n}" }
                                        }
                                    }
                                }
                            }
                            label { class: "check",
                                input { r#type: "checkbox", checked: p.back_cover, onchange: move |e| set_prefs(&|p| p.back_cover = e.checked()) }
                                span { "{t(lang, \"dialog.print.booklet.back_cover\")}" }
                            }
                            div { class: "hint", "{t(lang, \"dialog.print.booklet.hint\")}" }
                        },
                        Mode::Poster => rsx! {
                            div { class: "field-grid",
                                div { class: "field",
                                    label { r#for: "print-poster-scale", "{t(lang, \"dialog.print.poster.scale\")}" }
                                    input { id: "print-poster-scale", r#type: "number", min: "10", max: "1000", step: "10", value: "{p.poster_scale}",
                                        oninput: move |e| { let v = e.value().replace(',', ".").parse::<f32>().unwrap_or(100.0); set_prefs(&|p| p.poster_scale = v) } }
                                }
                                div { class: "field",
                                    label { r#for: "print-overlap", "{t(lang, \"dialog.print.poster.overlap\")}" }
                                    input { id: "print-overlap", r#type: "number", min: "0", max: "50", step: "1", value: "{p.overlap_mm}",
                                        oninput: move |e| { let v = e.value().replace(',', ".").parse::<f32>().unwrap_or(0.0).clamp(0.0, 200.0); set_prefs(&|p| p.overlap_mm = v) } }
                                }
                            }
                            label { class: "check",
                                input { r#type: "checkbox", checked: p.cut_marks, onchange: move |e| set_prefs(&|p| p.cut_marks = e.checked()) }
                                span { "{t(lang, \"dialog.print.poster.marks\")}" }
                            }
                            label { class: "check",
                                input { r#type: "checkbox", checked: p.labels, onchange: move |e| set_prefs(&|p| p.labels = e.checked()) }
                                span { "{t(lang, \"dialog.print.poster.labels\")}" }
                            }
                            if let Some((cols, rows)) = tiles {
                                div { class: "hint print-tiles", "{tf(lang, \"dialog.print.poster.tiles\", &[(\"cols\", &cols.to_string()), (\"rows\", &rows.to_string())])}" }
                            }
                        },
                    }}
                    div { class: "field-grid",
                        div { class: "field",
                            label { r#for: "print-orientation", "{t(lang, \"dialog.print.orientation\")}" }
                            select { id: "print-orientation", disabled: p.mode == Mode::Booklet,
                                onchange: move |e| { let v = e.value(); set_prefs(&|p| p.orientation = match v.as_str() { "portrait" => Orientation::Portrait, "landscape" => Orientation::Landscape, _ => Orientation::Auto }) },
                                option { value: "auto", selected: p.orientation == Orientation::Auto, "{t(lang, \"dialog.print.orientation.auto\")}" }
                                option { value: "portrait", selected: p.orientation == Orientation::Portrait, "{t(lang, \"dialog.print.orientation.portrait\")}" }
                                option { value: "landscape", selected: p.orientation == Orientation::Landscape, "{t(lang, \"dialog.print.orientation.landscape\")}" }
                            }
                        }
                        div { class: "field",
                            label { r#for: "print-paper", "{t(lang, \"dialog.print.paper\")}" }
                            select { id: "print-paper", class: "print-paper", onchange: move |e| paper_key.set(e.value()),
                                for m in media_now.iter() {
                                    option { key: "{m.key}", value: "{m.key}", selected: m.key == key_now, "{media_label(lang, m)}" }
                                }
                            }
                        }
                    }
                    div { class: "field-grid",
                        div { class: "field",
                            label { r#for: "print-sides", "{t(lang, \"dialog.print.sides\")}" }
                            select { id: "print-sides", class: "print-sides", disabled: sides_fixed || sides_allowed.len() < 2,
                                onchange: move |e| { let v = e.value(); sides.set(match v.as_str() { "long" => Sides::LongEdge, "short" => Sides::ShortEdge, _ => Sides::OneSided }) },
                                for (s, key, label) in side_choices.into_iter() {
                                    option { key: "{key}", value: "{key}", selected: shown_sides == s, "{label}" }
                                }
                            }
                        }
                        if shows_color {
                            div { class: "field",
                                label { r#for: "print-color", "{t(lang, \"dialog.print.color\")}" }
                                select { id: "print-color", onchange: move |e| gray.set(e.value() == "gray"),
                                    option { value: "color", selected: !gray(), "{t(lang, \"dialog.print.color.color\")}" }
                                    option { value: "gray", selected: gray(), "{t(lang, \"dialog.print.color.gray\")}" }
                                }
                            }
                        }
                    }
                    if is_windows && is_printer() {
                        div { class: "field",
                            label { r#for: "print-win-dpi", "{t(lang, \"dialog.print.resolution\")}" }
                            select { id: "print-win-dpi", onchange: move |e| { let v = e.value().parse::<u32>().unwrap_or(300); set_prefs(&|p| p.win_dpi = v) },
                                option { value: "300", selected: p.win_dpi == 300, "300 dpi" }
                                option { value: "600", selected: p.win_dpi == 600, "600 dpi" }
                                option { value: "0", selected: p.win_dpi == 0, "{t(lang, \"dialog.print.resolution.printer\")}" }
                            }
                        }
                    } else if dest != PDF {
                        div { class: "field-grid",
                            label { class: "check",
                                input { r#type: "checkbox", checked: p.as_image || low, disabled: low,
                                    onchange: move |e| set_prefs(&|p| p.as_image = e.checked()) }
                                span { "{t(lang, \"dialog.print.as_image\")}" }
                            }
                            if p.as_image && !low {
                                div { class: "field",
                                    select { title: "{t(lang, \"dialog.print.resolution\")}", onchange: move |e| { let v = e.value().parse::<u32>().unwrap_or(300); set_prefs(&|p| p.image_dpi = v) },
                                        for d in [150u32, 300, 600] {
                                            option { key: "{d}", value: "{d}", selected: p.image_dpi == d, "{d} dpi" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                // The preview.
                div { class: "print-preview",
                    div { class: "print-stage-wrap", tabindex: "0", title: "{t(lang, \"dialog.print.preview\")}",
                        onkeydown: move |e| match e.key() {
                            Key::ArrowRight => { e.prevent_default(); step(1) }
                            Key::ArrowLeft => { e.prevent_default(); step(-1) }
                            Key::Home => { e.prevent_default(); index.set(0) }
                            Key::End => { e.prevent_default(); index.set(total_sides().saturating_sub(1)) }
                            _ => {}
                        },
                        div { class: "print-stage-box",
                            div { class: "print-stage", style: "{stage_style}",
                                if let Some(g) = shown_gen {
                                    img { class: "print-sheet-img", src: "/sheet/p/{g}.png", alt: "{side_label}", draggable: "false" }
                                }
                                if show_area {
                                    div { class: "print-area", style: "{area_style}" }
                                }
                                if shown_gen.is_none() {
                                    div { class: "print-wait", "{t(lang, \"dialog.print.preparing\")}" }
                                }
                            }
                        }
                    }
                    if let Some(e) = preview_error() {
                        div { class: "hint print-hint warn", "{e}" }
                    }
                    div { class: "print-nav",
                        button { class: "ibtn", title: "{t(lang, \"dialog.print.first\")}", disabled: i == 0, onclick: move |_| index.set(0), Icon { name: "first" } }
                        button { class: "ibtn print-prev", title: "{t(lang, \"dialog.print.prev\")}", disabled: i == 0, onclick: move |_| step(-1), Icon { name: "prev" } }
                        span { class: "print-sheet-count", "data-count": "{total}", "data-index": "{i}", "{side_label}" }
                        button { class: "ibtn print-next", title: "{t(lang, \"dialog.print.next\")}", disabled: i + 1 >= total, onclick: move |_| step(1), Icon { name: "next" } }
                        button { class: "ibtn", title: "{t(lang, \"dialog.print.last\")}", disabled: i + 1 >= total, onclick: move |_| index.set(total_sides().saturating_sub(1)), Icon { name: "last" } }
                    }
                    {match &plans_now {
                        Ok(_) => rsx! { div { class: "print-summary", "{summary}" } },
                        Err(e) => rsx! { div { class: "print-error callout warn", "{e}" } },
                    }}
                    if !warnings.is_empty() {
                        div { class: "print-notes",
                            for (k, w) in warnings.iter().enumerate() {
                                div { key: "{k}", class: "print-warning", "{w}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The driver's own settings window (Windows): what is chosen there comes
/// back into the window.
#[component]
fn PropertiesButton(
    printer: String,
    sides: Signal<Sides>,
    gray: Signal<bool>,
    copies: Signal<String>,
    paper_key: Signal<String>,
) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    rsx! {
        button { class: "tbtn print-properties", title: "{t(lang, \"dialog.print.properties\")}",
            onclick: move |_| open_properties(st, printer.clone(), sides, gray, copies, paper_key),
            "{t(lang, \"dialog.print.properties\")}"
        }
    }
}

#[cfg(target_os = "windows")]
fn open_properties(
    st: AppState,
    printer: String,
    mut sides: Signal<Sides>,
    mut gray: Signal<bool>,
    mut copies: Signal<String>,
    mut paper_key: Signal<String>,
) {
    use dioxus::desktop::tao::platform::windows::WindowExtWindows;
    let hwnd = dioxus::desktop::window().window.hwnd();
    let chosen: Option<(String, Vec<u8>)> = (*PROPERTIES.peek()).clone();
    let current = chosen.filter(|(p, _)| *p == printer).map(|(_, b)| b);
    spawn(async move {
        let name = printer.clone();
        let chosen = tokio::task::spawn_blocking(move || {
            printing::windows::properties(hwnd, &name, current.as_deref())
        })
        .await;
        match chosen {
            Ok(Ok(Some(bytes))) => {
                let f = printing::devmode::read_fields(&bytes);
                sides.set(match f.duplex {
                    printing::devmode::DMDUP_VERTICAL => Sides::LongEdge,
                    printing::devmode::DMDUP_HORIZONTAL => Sides::ShortEdge,
                    _ => Sides::OneSided,
                });
                gray.set(f.color == printing::devmode::DMCOLOR_MONOCHROME);
                if f.copies > 0 {
                    copies.set(f.copies.to_string());
                }
                if f.paper > 0 && f.paper != printing::devmode::DMPAPER_USER {
                    paper_key.set(format!("dm:{}", f.paper));
                }
                *PROPERTIES.write() = Some((printer, bytes));
            }
            Ok(Ok(None)) => {}
            Ok(Err(e)) => st.toast(
                crate::state::ToastKind::Error,
                tf(st.lang(), "toast.print_failed", &[("reason", &e.to_string())]),
                "",
            ),
            Err(e) => st.error(e),
        }
    });
}

#[cfg(not(target_os = "windows"))]
fn open_properties(
    _st: AppState,
    _printer: String,
    _sides: Signal<Sides>,
    _gray: Signal<bool>,
    _copies: Signal<String>,
    _paper_key: Signal<String>,
) {
}

/// The settings chosen in a driver's window, for the jobs of that printer (Windows).
#[cfg(target_os = "windows")]
pub static PROPERTIES: GlobalSignal<Option<(String, Vec<u8>)>> = Signal::global(|| None);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_are_remembered_and_old_ones_filled_in() {
        let mut p = PrintPrefs {
            mode: Mode::Booklet,
            booklet_sides: BookletSides::FrontOnly,
            ..Default::default()
        };
        p.printers.insert(
            "Office".into(),
            PrinterPrefs {
                paper: "iso_a4_210x297mm".into(),
                sides: Some(Sides::LongEdge),
                gray: Some(true),
            },
        );
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"booklet\""));
        let back: PrintPrefs = serde_json::from_str(&json).unwrap();
        assert_eq!(back, p);
        // Settings saved by an older version: the rest are the defaults.
        let old: PrintPrefs = serde_json::from_str(r#"{"mode":"poster","overlap_mm":10}"#).unwrap();
        assert_eq!(old.mode, Mode::Poster);
        assert_eq!(old.overlap_mm, 10.0);
        assert_eq!(old.per_sheet, 2);
        assert!(old.collate);
        let bad: PrintPrefs = serde_json::from_str("not json").unwrap_or_default();
        assert_eq!(bad, PrintPrefs::default());
    }

    #[test]
    fn settings_make_the_options_of_the_plan() {
        let p = PrintPrefs {
            mode: Mode::Multiple,
            per_sheet: 4,
            gutter_mm: 10.0,
            orientation: Orientation::Landscape,
            ..Default::default()
        };
        let o = p.options(Paper::default(), Vec::new(), Sides::LongEdge);
        match o.layout {
            Layout::NUp {
                per_sheet, gutter, ..
            } => {
                assert_eq!(per_sheet, 4);
                assert!((gutter - 28.3465).abs() < 0.01);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(o.orientation, Orientation::Landscape);
        assert_eq!(o.sides, Sides::LongEdge);
        let b = PrintPrefs {
            mode: Mode::Booklet,
            orientation: Orientation::Portrait,
            ..Default::default()
        };
        assert_eq!(
            b.options(Paper::default(), Vec::new(), Sides::OneSided)
                .orientation,
            Orientation::Auto
        );
        let c = PrintPrefs {
            size: SizeChoice::Custom,
            scale: 150.0,
            ..Default::default()
        };
        assert!(matches!(c.layout(), Layout::Single { sizing: Sizing::Custom(s), .. } if s == 150.0));
    }

    #[test]
    fn the_paper_follows_what_was_chosen_before() {
        let media = standard_media();
        assert_eq!(
            choose_paper(&media, &["", "na_letter_8.5x11in"]),
            "na_letter_8.5x11in"
        );
        assert_eq!(
            choose_paper(&media, &["nope", "iso_a5_148x210mm"]),
            "iso_a5_148x210mm"
        );
        assert_eq!(choose_paper(&media, &["nope"]), media[0].key);
        assert_eq!(choose_paper(&[], &["x"]), "");
    }
}

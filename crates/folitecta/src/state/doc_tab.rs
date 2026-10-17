//! One open document tab and what its panes show and select.

use super::*;

/// Which comments are shown (a view setting: the document does not change).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CommentFilter {
    pub hidden_authors: BTreeSet<String>,
    pub hidden_types: BTreeSet<String>,
    /// Some(true): only checked comments; Some(false): only unchecked ones.
    pub checked: Option<bool>,
    /// Only comments with this review state ("" = without a state).
    pub status: Option<String>,
}

impl CommentFilter {
    pub fn is_empty(&self) -> bool {
        *self == CommentFilter::default()
    }

    /// Whether a top-level comment passes the filter.
    pub fn shows(&self, c: &CommentInfo) -> bool {
        !self.hidden_authors.contains(&c.author)
            && !self.hidden_types.contains(&c.subtype)
            && self.checked.is_none_or(|k| c.marked == k)
            && self.status.as_ref().is_none_or(|s| &c.review == s)
    }
}

#[derive(Debug, Clone, Default)]
pub struct SearchState {
    pub query: String,
    pub hits: Vec<SearchHit>,
    pub current: usize,
    pub case_sensitive: bool,
    pub whole_words: bool,
    /// Matches outside the page text (comments, bookmarks, fields...).
    pub other: Vec<folitecta_core::search::OtherHit>,
    /// Results of a search through the PDF files of a folder.
    pub folder: Option<FolderSearch>,
    /// Collapsed result groups.
    pub collapsed: HashSet<String>,
    /// The hits cover the whole query (not words of it found near each
    /// other): Find and Replace may step through and replace them.
    pub phrase: bool,
    /// The search (query and options) whose results Annotate Results
    /// already highlighted, so finding them again does not add more.
    pub annotated: Option<String>,
}

/// A search through the PDF files of a folder.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FolderSearch {
    pub root: PathBuf,
    pub query: String,
    pub files: usize,
    pub searched: usize,
    pub running: bool,
    pub hits: Vec<FolderHit>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FolderHit {
    pub path: PathBuf,
    pub page: usize,
    pub snippet: String,
}

/// One open document tab.
pub struct DocTab {
    pub id: u64,
    pub doc: Document,
    pub render: Arc<RenderService>,
    pub page: usize,
    pub zoom: Zoom,
    pub layout: Layout,
    pub view_rotation: i32,
    pub selected_pages: Vec<usize>,
    pub selected_annot: Option<(usize, usize)>,
    pub search: SearchState,
    pub expanded_bookmarks: HashSet<String>,
    pub page_sizes: Vec<PageSize>,
    pub revision: u64,
    pub highlight_fields: bool,
    /// The kinds of field highlighted (ids of `commands::forms::FIELD_TYPES`;
    /// empty: every kind), and whether only required fields are.
    pub highlight_kinds: Vec<&'static str>,
    pub highlight_required: bool,
    /// Current text selection (characters of one page).
    pub text_sel: Option<TextSel>,
    /// Ruler guides (view only, not saved in the PDF).
    pub guides: Vec<Guide>,
    /// Visited pages for previous/next view navigation.
    pub history: Vec<usize>,
    pub history_pos: usize,
    /// Comments shown on the pages and in the comments pane.
    pub comment_filter: CommentFilter,
    /// Bumped when the displayed pages change without a new revision (filters).
    pub view_gen: u64,
    /// Selected bookmark in the bookmarks pane.
    pub selected_bookmark: Option<Vec<usize>>,
    /// Selected named destination.
    pub selected_dest: Option<String>,
    /// Briefly highlighted area of a page (display points), e.g. a destination.
    pub flash: Option<(usize, [f32; 4])>,
    /// Signature validation results for a revision.
    pub sig_status: Option<(u64, Vec<folitecta_core::digsig::SignatureStatus>)>,
    /// Layer visibility of the view where it differs from the document's default.
    pub layer_view: HashMap<(u32, u16), bool>,
    /// Selected entries of the layers pane (tree paths).
    pub selected_layers: Vec<Vec<usize>>,
    /// Collapsed entries of the layers pane.
    pub collapsed_layers: HashSet<String>,
    /// Selected attachment (index in the attachments list).
    pub selected_attachment: Option<usize>,
    /// Selected form fields.
    pub selected_fields: Vec<(u32, u16)>,
    /// Collapsed name groups of the fields pane.
    pub collapsed_fields: HashSet<String>,
    /// Files view of a portfolio.
    pub portfolio: PortfolioState,
    /// Tags, reading order, content and accessibility check panes.
    pub acc: AccState,
    /// Picture selected with the pictures tool: page and index among the
    /// pictures of the page.
    pub image_sel: Option<(usize, usize)>,
}

/// What the accessibility panes show and select.
#[derive(Debug, Clone, Default)]
pub struct AccState {
    /// Selected element of the tags pane.
    pub tag: Option<(u32, u16)>,
    /// Collapsed elements of the tags pane.
    pub collapsed: HashSet<(u32, u16)>,
    /// Selected item of the order pane (on the current page).
    pub order: Option<usize>,
    /// Selected entry of the content pane: page and MCID (None: content outside tags).
    pub content: Option<(usize, Option<i64>)>,
    /// Last accessibility check.
    pub check: Option<CheckState>,
}

/// What a person decided about a rule of the accessibility check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Passed,
    Failed,
    Skipped,
}

/// Results of an accessibility check.
#[derive(Debug, Clone, Default)]
pub struct CheckState {
    pub results: Vec<folitecta_core::accessibility::CheckResult>,
    /// Rules passed, failed or skipped by hand.
    pub verdicts: HashMap<String, Verdict>,
    /// Selected rule and item.
    pub rule: Option<String>,
    pub item: Option<usize>,
    /// Expanded rules and collapsed categories.
    pub expanded: HashSet<String>,
    pub collapsed: HashSet<String>,
    /// When the check ran (shown in the report).
    pub when: String,
}

/// Where the files view of a portfolio is and what is selected in it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PortfolioState {
    /// The cover pages are shown instead of the files.
    pub cover: bool,
    /// Current folder (`None`: the root).
    pub folder: Option<i64>,
    pub back: Vec<Option<i64>>,
    pub forward: Vec<Option<i64>>,
    /// Selected files (name-tree keys) and folders.
    pub selected: Vec<Vec<u8>>,
    pub selected_folders: Vec<i64>,
    /// Selected column in the portfolio properties.
    pub column: Option<usize>,
}

impl PortfolioState {
    /// Open a folder, remembering where we were.
    pub fn open_folder(&mut self, folder: Option<i64>) {
        if folder != self.folder {
            self.back.push(self.folder);
            self.forward.clear();
            self.folder = folder;
            self.selected.clear();
            self.selected_folders.clear();
        }
    }
}

/// Hidden parent layers hide their children in the view (a view option).
pub static LAYER_INHERIT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

impl DocTab {
    pub fn new(id: u64, doc: Document) -> Self {
        // Bookmarks saved as open start expanded.
        fn open_keys(
            items: &[folitecta_core::OutlineItem],
            prefix: &mut Vec<usize>,
            out: &mut HashSet<String>,
        ) {
            for (i, item) in items.iter().enumerate() {
                prefix.push(i);
                if item.open && !item.children.is_empty() {
                    out.insert(prefix.iter().map(|x| x.to_string()).collect::<Vec<_>>().join("."));
                }
                open_keys(&item.children, prefix, out);
                prefix.pop();
            }
        }
        let mut expanded = HashSet::new();
        open_keys(&doc.outline(), &mut Vec::new(), &mut expanded);
        let render = Arc::new(RenderService::new(doc.bytes()));
        let page_sizes = doc.page_sizes();
        let revision = doc.revision();
        let mut tab = DocTab {
            id,
            doc,
            render,
            page: 0,
            zoom: Zoom::FitWidth,
            layout: Layout::Continuous,
            view_rotation: 0,
            selected_pages: Vec::new(),
            selected_annot: None,
            search: SearchState::default(),
            expanded_bookmarks: expanded,
            page_sizes,
            revision,
            highlight_fields: false,
            highlight_kinds: Vec::new(),
            highlight_required: false,
            text_sel: None,
            guides: Vec::new(),
            history: vec![0],
            history_pos: 0,
            comment_filter: CommentFilter::default(),
            view_gen: 0,
            selected_bookmark: None,
            selected_dest: None,
            flash: None,
            sig_status: None,
            layer_view: HashMap::new(),
            selected_layers: Vec::new(),
            collapsed_layers: HashSet::new(),
            selected_attachment: None,
            selected_fields: Vec::new(),
            collapsed_fields: HashSet::new(),
            portfolio: PortfolioState::default(),
            acc: AccState::default(),
            image_sel: None,
        };
        // A portfolio opens on its initial file, when it names one.
        if let Some(key) = tab.doc.portfolio().and_then(|p| p.initial) {
            tab.portfolio.selected = vec![key];
        }
        if LAYER_INHERIT.load(std::sync::atomic::Ordering::Relaxed) && !tab.doc.layers().is_empty() {
            tab.update_render();
        }
        tab
    }

    /// Layer visibility shown by the view (empty when it is the default one).
    pub fn view_layer_states(&self) -> HashMap<(u32, u16), bool> {
        let inherit = LAYER_INHERIT.load(std::sync::atomic::Ordering::Relaxed);
        if self.layer_view.is_empty() && !inherit {
            return HashMap::new();
        }
        let mut states = self.doc.layer_defaults();
        states.extend(self.layer_view.iter().map(|(k, v)| (*k, *v)));
        if inherit {
            folitecta_core::layers::inherit_visibility(&self.doc.layer_tree(), &mut states);
        }
        states
    }

    /// Whether a layer is shown in the view (its own state, parents aside).
    pub fn layer_visible(&self, id: (u32, u16), default: bool) -> bool {
        self.layer_view.get(&id).copied().unwrap_or(default)
    }

    /// Change the layer visibility of the view and redraw the pages.
    pub fn set_layer_view(&mut self, states: HashMap<(u32, u16), bool>) {
        let defaults = self.doc.layer_defaults();
        // Keep only what differs from the document.
        self.layer_view = states
            .into_iter()
            .filter(|(id, v)| defaults.get(id).is_some_and(|d| d != v))
            .collect();
        self.view_gen += 1;
        self.update_render();
    }

    /// Record a navigation to `page` in the view history.
    pub fn push_history(&mut self, page: usize) {
        if self.history.get(self.history_pos) == Some(&page) {
            return;
        }
        self.history.truncate(self.history_pos + 1);
        self.history.push(page);
        if self.history.len() > 200 {
            self.history.remove(0);
        }
        self.history_pos = self.history.len() - 1;
    }

    /// Call after any mutation of `doc`: refreshes caches and the renderer.
    pub fn refresh(&mut self) {
        if self.doc.revision() != self.revision {
            self.page_sizes = self.doc.page_sizes();
            self.revision = self.doc.revision();
            self.update_render();
            let n = self.doc.page_count();
            if self.page >= n {
                self.page = n.saturating_sub(1);
            }
            self.selected_pages.retain(|p| *p < n);
            self.selected_annot = None;
            self.text_sel = None;
            self.image_sel = None;
            // Search rectangles refer to the previous revision.
            self.search.hits.clear();
        }
    }

    /// Top-level comments the filter hides.
    pub fn filtered_out(&self) -> HashSet<(u32, u16)> {
        if self.comment_filter.is_empty() {
            return HashSet::new();
        }
        self.doc
            .comment_infos()
            .into_iter()
            .filter(|c| c.reply_to.is_none() && !c.is_state && !self.comment_filter.shows(c))
            .map(|c| c.id)
            .collect()
    }

    /// Give the renderer the document as displayed (comments hidden by the filter).
    pub fn update_render(&mut self) {
        let hidden = if self.comment_filter.is_empty() {
            HashSet::new()
        } else {
            self.filtered_out()
        };
        let layers = self.view_layer_states();
        let bytes = if hidden.is_empty() && layers.is_empty() {
            self.doc.bytes()
        } else {
            self.doc
                .view_bytes(&hidden, &layers)
                .unwrap_or_else(|_| self.doc.bytes())
        };
        self.render.replace(bytes);
    }

    /// Change the comment filter and redraw the pages.
    pub fn set_comment_filter(&mut self, filter: CommentFilter) {
        if filter != self.comment_filter {
            self.comment_filter = filter;
            self.view_gen += 1;
            self.update_render();
        }
    }

    /// Revision of the displayed pages, for page image addresses.
    pub fn view_rev(&self) -> u64 {
        self.revision
            .wrapping_mul(1000)
            .wrapping_add(self.view_gen % 1000)
    }

    pub fn title(&self) -> String {
        self.doc.display_name()
    }

    /// Pages targeted by page operations: the selection, or the current page.
    pub fn target_pages(&self) -> Vec<usize> {
        if self.selected_pages.is_empty() {
            vec![self.page]
        } else {
            self.selected_pages.clone()
        }
    }

    /// Display size of a page in points, after the view rotation.
    pub fn display_size(&self, page: usize) -> (f32, f32) {
        let s = self.page_sizes.get(page).copied().unwrap_or(PageSize {
            width: 595.0,
            height: 842.0,
            rotation: 0,
            media_box: [0.0; 4],
            crop_box: [0.0; 4],
        });
        if self.view_rotation % 180 != 0 {
            (s.height, s.width)
        } else {
            (s.width, s.height)
        }
    }
}

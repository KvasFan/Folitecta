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

/// A spell check under way (Spell Check and Replace): the words found and
/// the one shown.
#[derive(Debug, Clone, Default)]
pub struct SpellRun {
    /// The words the dictionaries do not know, in reading order.
    pub list: Vec<folitecta_core::Misspelling>,
    /// The word shown (index in `list`).
    pub current: Option<usize>,
    /// Where the check is (`Misspelling::key`), to find the word shown again
    /// when the list is made again.
    pub anchor: Option<(usize, usize)>,
    /// What the list was made for: the revision of the document, the
    /// checker (its address and changes) and what is checked.
    pub made_for: Option<(u64, usize, u64, folitecta_core::SpellScope)>,
    /// Words passed over with Ignore: place, position and spelling.
    pub passed: Vec<(folitecta_core::SpellPlace, usize, String)>,
}

/// What a view of a document shows: its page, zoom, layout and turn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewState {
    pub page: usize,
    pub zoom: Zoom,
    pub layout: Layout,
    pub rotation: i32,
}

/// A second view of the document, beside or below the first (Window ›
/// Split). The focused view is the one of the tab (`page`, `zoom`, ...), so
/// every command acts on it; the other one is kept here, and they change
/// places when the other one is clicked.
#[derive(Debug, Clone, PartialEq)]
pub struct SplitView {
    /// Side by side, or one above the other.
    pub side_by_side: bool,
    /// The focused view: 0 the first (left or top), 1 the second.
    pub focus: u8,
    /// The view not focused, and the size of its canvas.
    pub other: ViewState,
    pub other_size: (f64, f64),
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
    /// The second view of the document, when the window is split.
    pub split: Option<SplitView>,
    pub selected_pages: Vec<usize>,
    pub selected_annot: Option<(usize, usize)>,
    /// Comments added to the selected one with Shift or Ctrl (indices on its
    /// page), valid while `more_anchor` is the selected comment.
    pub more_annots: Vec<usize>,
    pub more_anchor: Option<(usize, usize)>,
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
    /// Bumped whenever the pages shown change (a new revision of the
    /// document or of the view): the revision in the addresses of the page
    /// pictures. It only grows, so an address is never used for two
    /// different pictures.
    pub view_serial: u64,
    /// For every page, the `view_serial` at which its picture last changed:
    /// the revision in the address of its picture. An edit of one page
    /// leaves the addresses (and the pictures kept) of the others.
    pub page_shown: Vec<u64>,
    /// Selected bookmark in the bookmarks pane.
    pub selected_bookmark: Option<Vec<usize>>,
    /// Selected named destination.
    pub selected_dest: Option<String>,
    /// Selected link of the links pane: page and index among the page's annotations.
    pub selected_link: Option<(usize, usize)>,
    /// Briefly highlighted area of a page (display points), e.g. a destination.
    pub flash: Option<(usize, [f32; 4])>,
    /// Signature validation results, and the state they are for (see
    /// `commands::signatures::status_key`).
    pub sig_status: Option<(
        crate::commands::signatures::StatusKey,
        Vec<folitecta_core::digsig::SignatureStatus>,
    )>,
    /// Revocation data fetched online for the document (only when the user
    /// turned online checks on), kept for the tab.
    pub revocation: folitecta_core::digsig::RevocationPool,
    /// Bumped when `revocation` changes.
    pub revocation_gen: u64,
    /// Layer visibility of the view where it differs from the document's default.
    pub layer_view: HashMap<(u32, u16), bool>,
    /// Selected entries of the layers pane (tree paths).
    pub selected_layers: Vec<Vec<usize>>,
    /// The selection of the layers pane the drawing layer was last chosen
    /// from: a single layer selected becomes the drawing layer.
    pub layer_sel_seen: Vec<Vec<usize>>,
    /// The name of a hidden layer the view showed because something was
    /// drawn on it, for a message.
    pub layer_shown: Option<String>,
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
    /// The spell check under way.
    pub spell: Option<SpellRun>,
    /// A redaction is being applied to the document (on another thread).
    pub redacting: bool,
    /// The last redaction applied, until a save checked it (dropped with the tab).
    pub redaction: Option<RedactionPending>,
}

/// A redaction applied to a document whose saved bytes are still to check.
#[derive(Debug, Clone)]
pub struct RedactionPending {
    /// The revision of the document right after the redaction.
    pub revision: u64,
    pub report: folitecta_core::redaction::RedactionReport,
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
            more_annots: Vec::new(),
            more_anchor: None,
            search: SearchState::default(),
            expanded_bookmarks: expanded,
            page_sizes,
            revision,
            highlight_fields: false,
            highlight_kinds: Vec::new(),
            highlight_required: false,
            split: None,
            text_sel: None,
            guides: Vec::new(),
            history: vec![0],
            history_pos: 0,
            comment_filter: CommentFilter::default(),
            view_gen: 0,
            view_serial: 0,
            page_shown: Vec::new(),
            selected_bookmark: None,
            selected_link: None,
            selected_dest: None,
            flash: None,
            sig_status: None,
            revocation: Default::default(),
            revocation_gen: 0,
            layer_view: HashMap::new(),
            selected_layers: Vec::new(),
            layer_sel_seen: Vec::new(),
            layer_shown: None,
            collapsed_layers: HashSet::new(),
            selected_attachment: None,
            selected_fields: Vec::new(),
            collapsed_fields: HashSet::new(),
            portfolio: PortfolioState::default(),
            acc: AccState::default(),
            image_sel: None,
            spell: None,
            redacting: false,
            redaction: None,
        };
        tab.page_shown = vec![0; tab.page_sizes.len()];
        // A portfolio opens on its initial file, when it names one.
        if let Some(key) = tab.doc.portfolio().and_then(|p| p.initial) {
            tab.portfolio.selected = vec![key];
        }
        // Comments of hidden layers, and the view option of parent layers.
        if !tab.doc.layers().is_empty() {
            tab.update_render();
        }
        tab
    }

    /// Annotations the hidden layers of the view hide.
    pub fn layer_hidden_annots(&self) -> HashSet<(u32, u16)> {
        self.doc.annotations_in_hidden_layers(&self.view_layer_states())
    }

    /// Indices of the annotations of a page the hidden layers of the view hide.
    pub fn layer_hidden_on_page(&self, page: usize) -> HashSet<usize> {
        self.doc
            .page_annotations_in_hidden_layers(page, &self.view_layer_states())
    }

    /// A single layer selected in the layers pane becomes the layer new
    /// drawings and text go to (other selections leave it as it is).
    fn follow_layer_selection(&mut self) {
        if self.selected_layers == self.layer_sel_seen {
            return;
        }
        self.layer_sel_seen = self.selected_layers.clone();
        if let [path] = self.selected_layers.as_slice() {
            let tree = self.doc.layer_tree();
            if let Some(id) = folitecta_core::layers::node_at(&tree, path).and_then(|n| n.layer_id()) {
                let _ = self.doc.set_drawing_layer(Some(id));
            }
        }
    }

    /// Show in the view a hidden layer something was just drawn on, so
    /// the user sees it (the document keeps its default visibility).
    fn show_layer_drawn(&mut self) {
        let Some(id) = self.doc.take_layer_drawn() else {
            return;
        };
        let mut states = self.doc.layer_defaults();
        states.extend(self.layer_view.iter().map(|(k, v)| (*k, *v)));
        let inherit = LAYER_INHERIT.load(std::sync::atomic::Ordering::Relaxed);
        let mut shown = states.clone();
        if inherit {
            folitecta_core::layers::inherit_visibility(&self.doc.layer_tree(), &mut shown);
        }
        if shown.get(&id).copied().unwrap_or(true) {
            return;
        }
        states.insert(id, true);
        if inherit {
            // The layers above it too, as hidden parents hide their children.
            for p in layer_ancestors(&self.doc.layer_tree(), id) {
                states.insert(p, true);
            }
        }
        self.layer_shown = self.doc.layers().into_iter().find(|l| l.id == id).map(|l| l.name);
        // Drawn again by the refresh under way.
        let defaults = self.doc.layer_defaults();
        self.layer_view = states
            .into_iter()
            .filter(|(id, v)| defaults.get(id).is_some_and(|d| d != v))
            .collect();
        self.view_gen += 1;
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
        self.bump_view();
        self.update_render();
    }

    /// The pages shown change without a new revision of the document.
    pub fn bump_view(&mut self) {
        self.view_gen += 1;
        self.view_serial += 1;
        self.page_shown = vec![self.view_serial; self.page_sizes.len()];
        self.render.set_page_revs(self.page_shown.clone());
    }

    /// The revision in the address of the picture of `page`.
    pub fn page_rev(&self, page: usize) -> u64 {
        self.page_shown.get(page).copied().unwrap_or(self.view_serial)
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
        self.follow_layer_selection();
        if self.doc.revision() != self.revision {
            self.show_layer_drawn();
            let seen = self.revision;
            self.page_sizes = self.doc.page_sizes();
            self.revision = self.doc.revision();
            self.view_serial += 1;
            // The pages whose picture changed since the revision seen get
            // new addresses; all of them when pages came or went.
            let n = self.page_sizes.len();
            if self.page_shown.len() != n || seen == u64::MAX {
                self.page_shown = vec![self.view_serial; n];
            } else {
                for p in 0..n {
                    if self.doc.page_revision(p) > seen {
                        self.page_shown[p] = self.view_serial;
                    }
                }
            }
            self.update_render();
            let n = self.doc.page_count();
            if self.page >= n {
                self.page = n.saturating_sub(1);
            }
            if let Some(s) = self.split.as_mut() {
                s.other.page = s.other.page.min(n.saturating_sub(1));
            }
            self.selected_pages.retain(|p| *p < n);
            self.selected_annot = None;
            self.text_sel = None;
            self.image_sel = None;
            self.selected_link = None;
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
        // The comments of hidden layers are hidden too (by the default
        // visibility when the view does not change it; without layers the
        // bytes of the document themselves).
        let bytes = self
            .doc
            .view_bytes(&hidden, &layers)
            .unwrap_or_else(|_| self.doc.bytes());
        if self.page_shown.len() != self.page_sizes.len() {
            self.page_shown = vec![self.view_serial; self.page_sizes.len()];
        }
        self.render.replace_pages(bytes, self.page_shown.clone());
    }

    /// Change the comment filter and redraw the pages.
    pub fn set_comment_filter(&mut self, filter: CommentFilter) {
        if filter != self.comment_filter {
            self.comment_filter = filter;
            self.bump_view();
            self.update_render();
        }
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
        self.display_size_turned(page, self.view_rotation)
    }

    /// The size of a page as a view turned by `rotation` shows it.
    pub fn display_size_turned(&self, page: usize, rotation: i32) -> (f32, f32) {
        let s = self.page_sizes.get(page).copied().unwrap_or(PageSize {
            width: 595.0,
            height: 842.0,
            rotation: 0,
            media_box: [0.0; 4],
            crop_box: [0.0; 4],
        });
        if rotation % 180 != 0 {
            (s.height, s.width)
        } else {
            (s.width, s.height)
        }
    }

    /// The comments chosen on the page of the selected one: the selected one
    /// first (the one the others are matched to), then those added with
    /// Shift or Ctrl.
    pub fn chosen_annots(&self) -> Option<(usize, Vec<usize>)> {
        let (page, first) = self.selected_annot?;
        let mut list = vec![first];
        if self.more_anchor == self.selected_annot {
            list.extend(self.more_annots.iter().copied().filter(|i| *i != first));
        }
        Some((page, list))
    }

    /// Choose these comments of a page (the first is the selected one).
    pub fn set_chosen(&mut self, page: usize, list: &[usize]) {
        self.selected_annot = list.first().map(|i| (page, *i));
        self.more_annots = list.iter().skip(1).copied().collect();
        self.more_anchor = self.selected_annot;
    }

    /// The focused view.
    pub fn view_state(&self) -> ViewState {
        ViewState {
            page: self.page,
            zoom: self.zoom,
            layout: self.layout,
            rotation: self.view_rotation,
        }
    }

    /// Make `view` the focused view.
    pub fn set_view_state(&mut self, view: ViewState) {
        self.page = view.page;
        self.zoom = view.zoom;
        self.layout = view.layout;
        self.view_rotation = view.rotation;
    }
}

/// The layers above layer `id` in the layer tree.
fn layer_ancestors(nodes: &[folitecta_core::LayerNode], id: (u32, u16)) -> Vec<(u32, u16)> {
    fn walk(nodes: &[folitecta_core::LayerNode], id: (u32, u16), above: &mut Vec<(u32, u16)>) -> bool {
        for n in nodes {
            if n.layer_id() == Some(id) {
                return true;
            }
            let own = n.layer_id();
            if let Some(o) = own {
                above.push(o);
            }
            if walk(n.children(), id, above) {
                return true;
            }
            if own.is_some() {
                above.pop();
            }
        }
        false
    }
    let mut above = Vec::new();
    if walk(nodes, id, &mut above) {
        above
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The revision in the address of the picture of every page.
    fn revs(t: &DocTab) -> Vec<u64> {
        (0..t.doc.page_count()).map(|p| t.page_rev(p)).collect()
    }

    #[test]
    fn the_addresses_of_the_page_pictures_only_grow() {
        let bytes = folitecta_core::testdocs::long_doc(3);
        let doc = folitecta_core::Document::from_bytes(bytes, None, None).unwrap();
        let mut t = DocTab::new(1, doc);
        // More than a thousand changes of the view filters: every page gets
        // a new, larger address each time (it never starts again).
        let hide = CommentFilter {
            checked: Some(true),
            ..CommentFilter::default()
        };
        let mut last = revs(&t);
        for i in 0..1200 {
            let filter = if i % 2 == 0 {
                hide.clone()
            } else {
                CommentFilter::default()
            };
            t.set_comment_filter(filter);
            let now = revs(&t);
            assert!(
                now.iter().zip(&last).all(|(n, l)| n > l),
                "filter change {i}: {last:?} then {now:?}"
            );
            last = now;
        }
        // A first edit (after opening, every page counts as changed).
        t.doc
            .add_note(2, 300.0, 300.0, "First", "Reviewer", [1.0, 0.8, 0.2])
            .unwrap();
        t.refresh();
        let mut last = revs(&t);
        // An edit of page 1, its undo and its redo: page 1 gets a new,
        // larger address each time, the others keep theirs.
        for step in ["edit", "undo", "redo"] {
            match step {
                "edit" => {
                    t.doc
                        .add_note(1, 200.0, 200.0, "A note", "Reviewer", [1.0, 0.8, 0.2])
                        .unwrap();
                }
                "undo" => t.doc.undo().unwrap(),
                _ => t.doc.redo().unwrap(),
            }
            t.refresh();
            let now = revs(&t);
            assert!(now[1] > last[1], "{step}: page 1, {last:?} then {now:?}");
            assert_eq!((now[0], now[2]), (last[0], last[2]), "{step}: the other pages");
            last = now;
        }
    }
}

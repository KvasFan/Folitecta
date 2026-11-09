//! The vector paths of a page (FOL-L021): the shapes its content draws
//! (lines, curves, rectangles; filled, stroked or both), with their nodes on
//! the page, colours and line width, found by walking the content with its
//! graphics state. A node moved, a colour or width changed, a path deleted
//! changes the content itself (one undo step each), so the change stays when
//! the file is opened again. Clipping paths and the paths of reused
//! drawings (form XObjects) are not offered.

use hayro::kurbo::{Affine, Point};
use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Object, Stream};

use crate::document::Document;
use crate::error::{Error, Result};

/// A node of a path: where it is on the page, and where it is written.
#[derive(Debug, Clone, PartialEq)]
pub struct PathNode {
    /// On the page (PDF user space of the page).
    pub at: [f32; 2],
    /// The operation and the index of its x operand.
    op: usize,
    operand: usize,
    /// The node is the far corner of a rectangle (`re`): it moves the size.
    corner: bool,
}

/// What a node of a path becomes (Edit Paths › Convert).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    /// The segments on both sides of it straight lines.
    Straight,
    /// A sharp corner: its own handles drawn into it, the curves kept.
    Corner,
    /// A smooth node: its handles on one line through it, straight
    /// segments beside it made curves.
    Smooth,
}

/// A shape drawn with three clicks (Edit › Path Editing › 3 Point).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreePoint {
    /// From the first point through the second to the third.
    Arc,
    /// Through the three points.
    Circle,
    /// One side from the first point to the second, as high as the third.
    Rect,
}

/// The circle through three points: centre and radius; None when they are
/// on one line.
fn circle_through(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> Option<([f64; 2], f64)> {
    let d = 2.0 * (a[0] * (b[1] - c[1]) + b[0] * (c[1] - a[1]) + c[0] * (a[1] - b[1]));
    let size = (b[0] - a[0]).abs() + (b[1] - a[1]).abs() + (c[0] - a[0]).abs() + (c[1] - a[1]).abs();
    if d.abs() < 1e-6 * size.max(1.0) * size.max(1.0) {
        return None;
    }
    let sq = |p: [f64; 2]| p[0] * p[0] + p[1] * p[1];
    let x = (sq(a) * (b[1] - c[1]) + sq(b) * (c[1] - a[1]) + sq(c) * (a[1] - b[1])) / d;
    let y = (sq(a) * (c[0] - b[0]) + sq(b) * (a[0] - c[0]) + sq(c) * (b[0] - a[0])) / d;
    let r = ((a[0] - x).powi(2) + (a[1] - y).powi(2)).sqrt();
    Some(([x, y], r))
}

/// Curves (`c` operands) along a circle from angle `from`, `sweep` radians
/// (negative: clockwise), in pieces of at most a quarter turn.
fn arc_curves(centre: [f64; 2], r: f64, from: f64, sweep: f64) -> Vec<[f64; 6]> {
    let n = (sweep.abs() / std::f64::consts::FRAC_PI_2).ceil().max(1.0) as usize;
    let step = sweep / n as f64;
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    let at = |t: f64| [centre[0] + r * t.cos(), centre[1] + r * t.sin()];
    (0..n)
        .map(|i| {
            let (t0, t1) = (from + step * i as f64, from + step * (i + 1) as f64);
            let (p0, p1) = (at(t0), at(t1));
            [
                p0[0] - k * r * t0.sin(),
                p0[1] + k * r * t0.cos(),
                p1[0] + k * r * t1.sin(),
                p1[1] - k * r * t1.cos(),
                p1[0],
                p1[1],
            ]
        })
        .collect()
}

/// The operations of `shape` through `points` (PDF user space).
fn three_point_ops(shape: ThreePoint, points: [[f32; 2]; 3]) -> Result<Vec<Operation>> {
    let [a, b, c] = points.map(|p| [p[0] as f64, p[1] as f64]);
    let real = |v: &[f64]| v.iter().map(|x| Object::Real(*x as f32)).collect::<Vec<_>>();
    let on_a_line = || Error::Invalid("the three points are on one line".into());
    let mut ops = Vec::new();
    match shape {
        ThreePoint::Rect => {
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let len = (dx * dx + dy * dy).sqrt();
            if len < 1e-6 {
                return Err(on_a_line());
            }
            let n = [-dy / len, dx / len];
            let h = (c[0] - a[0]) * n[0] + (c[1] - a[1]) * n[1];
            if h.abs() < 1e-3 {
                return Err(on_a_line());
            }
            let up = |p: [f64; 2]| [p[0] + n[0] * h, p[1] + n[1] * h];
            ops.push(Operation::new("m", real(&a)));
            for p in [b, up(b), up(a)] {
                ops.push(Operation::new("l", real(&p)));
            }
            ops.push(Operation::new("h", Vec::new()));
        }
        ThreePoint::Circle => {
            let (o, r) = circle_through(a, b, c).ok_or_else(on_a_line)?;
            ops.push(Operation::new("m", real(&[o[0] + r, o[1]])));
            for v in arc_curves(o, r, 0.0, std::f64::consts::TAU) {
                ops.push(Operation::new("c", real(&v)));
            }
            ops.push(Operation::new("h", Vec::new()));
        }
        ThreePoint::Arc => {
            let (o, r) = circle_through(a, b, c).ok_or_else(on_a_line)?;
            let angle = |p: [f64; 2]| (p[1] - o[1]).atan2(p[0] - o[0]);
            let tau = std::f64::consts::TAU;
            let turn = |from: f64, to: f64| (to - from).rem_euclid(tau);
            let (ta, tb, tc) = (angle(a), angle(b), angle(c));
            // Anticlockwise when the second point is on the way, else clockwise.
            let ccw = turn(ta, tc);
            let sweep = if turn(ta, tb) <= ccw { ccw } else { ccw - tau };
            ops.push(Operation::new("m", real(&a)));
            for v in arc_curves(o, r, ta, sweep) {
                ops.push(Operation::new("c", real(&v)));
            }
        }
    }
    Ok(ops)
}

/// A path the page content draws.
#[derive(Debug, Clone, PartialEq)]
pub struct PagePath {
    /// Order among the paths of the page.
    pub index: usize,
    pub nodes: Vec<PathNode>,
    /// Bounds on the page: x0, y0, x1, y1.
    pub bounds: [f32; 4],
    pub stroked: bool,
    pub filled: bool,
    /// Device colours when they are known (RGB, grey or CMYK made RGB).
    pub stroke: Option<[f32; 3]>,
    pub fill: Option<[f32; 3]>,
    pub width: f32,
    /// The operations of the path: its first construction and its painting.
    first: usize,
    last: usize,
    /// The inverse of the matrix it is drawn with.
    to_user: [f64; 6],
}

impl PagePath {
    /// The upright rectangles the path is made of (PDF user space): each of
    /// its `re`, or the path itself when it joins four corners with
    /// straight lines; none when it is anything else.
    pub fn rectangles(&self) -> Vec<[f32; 4]> {
        let of = |a: [f32; 2], b: [f32; 2]| [a[0].min(b[0]), a[1].min(b[1]), a[0].max(b[0]), a[1].max(b[1])];
        let boxes: Vec<[f32; 4]> = self
            .nodes
            .windows(2)
            .filter(|w| w[1].corner && w[0].op == w[1].op)
            .map(|w| of(w[0].at, w[1].at))
            .collect();
        if !boxes.is_empty() {
            // Only rectangles: no other line or curve in the path.
            return if boxes.len() * 2 == self.nodes.len() {
                boxes
            } else {
                Vec::new()
            };
        }
        if !(4..=5).contains(&self.nodes.len()) {
            return Vec::new();
        }
        let b = self.bounds;
        let near = |v: f32, a: f32| (v - a).abs() < 0.5;
        let mut corners: Vec<(bool, bool)> = Vec::new();
        for n in &self.nodes {
            let (x, y) = (n.at[0], n.at[1]);
            if !(near(x, b[0]) || near(x, b[2])) || !(near(y, b[1]) || near(y, b[3])) {
                return Vec::new();
            }
            let c = (near(x, b[2]), near(y, b[3]));
            if !corners.contains(&c) {
                corners.push(c);
            }
        }
        if corners.len() == 4 { vec![b] } else { Vec::new() }
    }
}

#[derive(Clone, Copy)]
struct State {
    ctm: Affine,
    stroke: Option<[f32; 3]>,
    fill: Option<[f32; 3]>,
    width: f32,
}

fn num(o: &Object) -> Option<f64> {
    match o {
        Object::Integer(i) => Some(*i as f64),
        Object::Real(r) => Some(*r as f64),
        _ => None,
    }
}

fn nums(ops: &[Object]) -> Option<Vec<f64>> {
    ops.iter().map(num).collect()
}

fn colour(op: &str, v: &[f64]) -> Option<[f32; 3]> {
    let f = |x: f64| x.clamp(0.0, 1.0) as f32;
    match (op, v) {
        ("g" | "G", [g]) => Some([f(*g); 3]),
        ("rg" | "RG", [r, g, b]) => Some([f(*r), f(*g), f(*b)]),
        ("k" | "K", [c, m, y, k]) => {
            let x = |a: f64| f((1.0 - a) * (1.0 - k));
            Some([x(*c), x(*m), x(*y)])
        }
        _ => None,
    }
}

/// The paths of decoded content, with the page's starting matrix `base`.
fn find_paths(content: &Content, base: Affine) -> Vec<PagePath> {
    let mut out = Vec::new();
    let mut state = State {
        ctm: base,
        stroke: Some([0.0; 3]),
        fill: Some([0.0; 3]),
        width: 1.0,
    };
    let mut saved: Vec<State> = Vec::new();
    let mut nodes: Vec<PathNode> = Vec::new();
    let mut first: Option<usize> = None;
    let mut clip = false;
    let mut in_text = false;
    for (i, op) in content.operations.iter().enumerate() {
        let o = op.operator.as_str();
        let point = |x: f64, y: f64, ctm: Affine| {
            let p = ctm * Point::new(x, y);
            [p.x as f32, p.y as f32]
        };
        match o {
            "BT" => in_text = true,
            "ET" => in_text = false,
            "q" => saved.push(state),
            "Q" => {
                if let Some(s) = saved.pop() {
                    state = s;
                }
            }
            "cm" => {
                if let Some([a, b, c, d, e, f]) = nums(&op.operands).as_deref() {
                    state.ctm *= Affine::new([*a, *b, *c, *d, *e, *f]);
                }
            }
            "w" => {
                if let Some(w) = op.operands.first().and_then(num) {
                    state.width = w as f32;
                }
            }
            "g" | "rg" | "k" => {
                if let Some(c) = nums(&op.operands).and_then(|v| colour(o, &v)) {
                    state.fill = Some(c);
                }
            }
            "G" | "RG" | "K" => {
                if let Some(c) = nums(&op.operands).and_then(|v| colour(o, &v)) {
                    state.stroke = Some(c);
                }
            }
            "cs" | "scn" | "sc" => state.fill = None,
            "CS" | "SCN" | "SC" => state.stroke = None,
            "m" | "l" | "c" | "v" | "y" | "re" if !in_text => {
                let Some(v) = nums(&op.operands) else { continue };
                first.get_or_insert(i);
                match o {
                    "re" if v.len() == 4 => {
                        nodes.push(PathNode {
                            at: point(v[0], v[1], state.ctm),
                            op: i,
                            operand: 0,
                            corner: false,
                        });
                        nodes.push(PathNode {
                            at: point(v[0] + v[2], v[1] + v[3], state.ctm),
                            op: i,
                            operand: 2,
                            corner: true,
                        });
                    }
                    _ => {
                        for k in (0..v.len().saturating_sub(1)).step_by(2) {
                            nodes.push(PathNode {
                                at: point(v[k], v[k + 1], state.ctm),
                                op: i,
                                operand: k,
                                corner: false,
                            });
                        }
                    }
                }
            }
            "W" | "W*" => clip = true,
            "S" | "s" | "f" | "F" | "f*" | "B" | "B*" | "b" | "b*" | "n" => {
                if let Some(start) = first.take() {
                    let painted = o != "n";
                    if painted && !clip && !nodes.is_empty() {
                        let xs = nodes.iter().map(|n| n.at[0]);
                        let ys = nodes.iter().map(|n| n.at[1]);
                        let bounds = [
                            xs.clone().fold(f32::MAX, f32::min),
                            ys.clone().fold(f32::MAX, f32::min),
                            xs.fold(f32::MIN, f32::max),
                            ys.fold(f32::MIN, f32::max),
                        ];
                        let stroked = matches!(o, "S" | "s" | "B" | "B*" | "b" | "b*");
                        let filled = matches!(o, "f" | "F" | "f*" | "B" | "B*" | "b" | "b*");
                        out.push(PagePath {
                            index: out.len(),
                            nodes: std::mem::take(&mut nodes),
                            bounds,
                            stroked,
                            filled,
                            stroke: state.stroke,
                            fill: state.fill,
                            width: state.width,
                            first: start,
                            last: i,
                            to_user: state.ctm.inverse().as_coeffs(),
                        });
                    }
                }
                nodes.clear();
                clip = false;
            }
            _ => {}
        }
    }
    out
}

impl Document {
    /// The decoded content of page `page` and the matrix of its user space.
    fn page_content_ops(&self, page: usize) -> Result<(Content, Affine)> {
        let id = self.page_id(page)?;
        let data = self.inner.get_page_content(id);
        let content = Content::decode(&data)
            .map_err(|e| Error::Invalid(format!("the page content cannot be read: {e}")))?;
        Ok((content, Affine::IDENTITY))
    }

    /// The paths page `page` draws (painted ones, not clipping paths).
    pub fn page_paths(&self, page: usize) -> Result<Vec<PagePath>> {
        let (content, base) = self.page_content_ops(page)?;
        Ok(find_paths(&content, base))
    }

    /// The points of page `page` that drawings can be snapped to (PDF user
    /// space): the nodes of its paths, the four corners of its rectangles.
    pub fn snap_points(&self, page: usize) -> Vec<[f32; 2]> {
        let mut out = Vec::new();
        for p in self.page_paths(page).unwrap_or_default() {
            let rects = p.rectangles();
            if rects.is_empty() {
                out.extend(p.nodes.iter().map(|n| n.at));
            } else {
                for r in rects {
                    out.extend([[r[0], r[1]], [r[2], r[1]], [r[2], r[3]], [r[0], r[3]]]);
                }
            }
        }
        out.dedup();
        out
    }

    /// The topmost path of page `page` near `(x, y)` (page space), within
    /// `tolerance` points of its bounds.
    pub fn path_at(&self, page: usize, x: f32, y: f32, tolerance: f32) -> Option<PagePath> {
        let paths = self.page_paths(page).ok()?;
        paths
            .into_iter()
            .rev()
            .filter(|p| {
                let b = p.bounds;
                x >= b[0] - tolerance
                    && x <= b[2] + tolerance
                    && y >= b[1] - tolerance
                    && y <= b[3] + tolerance
            })
            .min_by(|a, b| {
                let area = |p: &PagePath| {
                    (p.bounds[2] - p.bounds[0]).max(1.0) * (p.bounds[3] - p.bounds[1]).max(1.0)
                };
                area(a).total_cmp(&area(b))
            })
    }

    /// Write `content` as the content of page `page` (one stream), one undo step.
    fn write_page_content(&mut self, page: usize, content: &Content) -> Result<()> {
        let id = self.page_id(page)?;
        let data = content
            .encode()
            .map_err(|e| Error::Invalid(format!("the page content cannot be written: {e}")))?;
        self.begin_edit();
        let contents: Vec<lopdf::ObjectId> = match self.inner.get_dictionary(id)?.get(b"Contents") {
            Ok(Object::Reference(r)) => vec![*r],
            Ok(Object::Array(a)) => a.iter().filter_map(|x| x.as_reference().ok()).collect(),
            _ => Vec::new(),
        };
        let mut stream = Stream::new(Dictionary::new(), data);
        let _ = stream.compress();
        match contents.first() {
            Some(first) => {
                self.inner.objects.insert(*first, Object::Stream(stream));
                for other in &contents[1..] {
                    self.inner
                        .objects
                        .insert(*other, Object::Stream(Stream::new(Dictionary::new(), Vec::new())));
                }
            }
            None => {
                let sid = self.inner.add_object(stream);
                self.inner
                    .get_dictionary_mut(id)?
                    .set("Contents", Object::Reference(sid));
            }
        }
        self.commit()
    }

    /// Move node `node` of path `path` of page `page` to `to` (page space).
    pub fn move_path_node(&mut self, page: usize, path: usize, node: usize, to: [f32; 2]) -> Result<()> {
        let (mut content, base) = self.page_content_ops(page)?;
        let paths = find_paths(&content, base);
        let p = paths
            .get(path)
            .ok_or_else(|| Error::Invalid("no such path".into()))?;
        let n = p
            .nodes
            .get(node)
            .ok_or_else(|| Error::Invalid("no such node".into()))?;
        let user = Affine::new(p.to_user) * Point::new(to[0] as f64, to[1] as f64);
        let op = &mut content.operations[n.op];
        if n.corner {
            // The far corner of a rectangle: its width and height.
            let v = nums(&op.operands).ok_or_else(|| Error::Invalid("a rectangle without numbers".into()))?;
            op.operands[2] = Object::Real((user.x - v[0]) as f32);
            op.operands[3] = Object::Real((user.y - v[1]) as f32);
        } else {
            op.operands[n.operand] = Object::Real(user.x as f32);
            op.operands[n.operand + 1] = Object::Real(user.y as f32);
        }
        self.write_page_content(page, &content)
    }

    /// Add a node to path `path` of page `page` at `at` (PDF user space of
    /// the page), after node `after`: a straight segment to it, from where
    /// the segment holding that node ends. Not for rectangles. One undo step.
    pub fn add_path_node(&mut self, page: usize, path: usize, after: usize, at: [f32; 2]) -> Result<()> {
        let (mut content, base) = self.page_content_ops(page)?;
        let paths = find_paths(&content, base);
        let p = paths
            .get(path)
            .ok_or_else(|| Error::Invalid("no such path".into()))?;
        let n = p
            .nodes
            .get(after)
            .ok_or_else(|| Error::Invalid("no such node".into()))?;
        if content.operations[n.op].operator == "re" {
            return Err(Error::Invalid("a rectangle has no nodes to add".into()));
        }
        let user = Affine::new(p.to_user) * Point::new(at[0] as f64, at[1] as f64);
        content.operations.insert(
            n.op + 1,
            Operation::new(
                "l",
                vec![Object::Real(user.x as f32), Object::Real(user.y as f32)],
            ),
        );
        self.write_page_content(page, &content)
    }

    /// Remove node `node` of path `path` of page `page`: the segment that
    /// ends there goes, its neighbours are joined (the first node: the path
    /// starts at the next one). Not for rectangles; a path keeps two nodes.
    /// One undo step.
    pub fn remove_path_node(&mut self, page: usize, path: usize, node: usize) -> Result<()> {
        let (mut content, base) = self.page_content_ops(page)?;
        let paths = find_paths(&content, base);
        let p = paths
            .get(path)
            .ok_or_else(|| Error::Invalid("no such path".into()))?;
        let n = p
            .nodes
            .get(node)
            .ok_or_else(|| Error::Invalid("no such node".into()))?;
        let mut ops: Vec<usize> = p.nodes.iter().map(|n| n.op).collect();
        ops.dedup();
        if ops.len() <= 2 {
            return Err(Error::Invalid(
                "a path keeps at least two nodes: delete the path instead".into(),
            ));
        }
        match content.operations[n.op].operator.as_str() {
            "re" => return Err(Error::Invalid("a rectangle has no nodes to remove".into())),
            "m" => {
                // The next segment, a line, starts the path instead.
                let next = content.operations.get_mut(n.op + 1);
                match next {
                    Some(op) if op.operator == "l" => op.operator = "m".into(),
                    _ => return Err(Error::Invalid("the first node of a curve stays".into())),
                }
            }
            _ => {}
        }
        content.operations.remove(n.op);
        self.write_page_content(page, &content)
    }

    /// Convert node `node` of path `path` of page `page` (a handle of a
    /// curve stands for the node it belongs to) to `kind`. Not for
    /// rectangles. One undo step.
    pub fn convert_path_node(&mut self, page: usize, path: usize, node: usize, kind: NodeKind) -> Result<()> {
        let (mut content, base) = self.page_content_ops(page)?;
        let paths = find_paths(&content, base);
        let p = paths
            .get(path)
            .ok_or_else(|| Error::Invalid("no such path".into()))?;
        let n = p
            .nodes
            .get(node)
            .ok_or_else(|| Error::Invalid("no such node".into()))?;
        let ops = &mut content.operations;
        // The operation that ends at the node.
        let at = match (ops[n.op].operator.as_str(), n.operand) {
            ("re", _) => return Err(Error::Invalid("the corners of a rectangle stay corners".into())),
            ("c", 0) | ("y", 0) => {
                n.op.checked_sub(1)
                    .ok_or_else(|| Error::Invalid("a curve without a start".into()))?
            }
            _ => n.op,
        };
        let segment = |o: &Operation| matches!(o.operator.as_str(), "l" | "c" | "v" | "y");
        let end = |o: &Operation| -> Option<[f64; 2]> {
            let v = nums(&o.operands)?;
            (v.len() >= 2).then(|| [v[v.len() - 2], v[v.len() - 1]])
        };
        // Curves written short (`v`, `y`) written whole, from their start.
        let whole = |ops: &mut Vec<Operation>, k: usize| -> Option<()> {
            let start = end(&ops[k.checked_sub(1)?])?;
            let v = nums(&ops[k].operands)?;
            let full = match (ops[k].operator.as_str(), v.as_slice()) {
                ("v", [x2, y2, x3, y3]) => vec![start[0], start[1], *x2, *y2, *x3, *y3],
                ("y", [x1, y1, x3, y3]) => vec![*x1, *y1, *x3, *y3, *x3, *y3],
                _ => return Some(()),
            };
            ops[k] = Operation::new("c", full.into_iter().map(|x| Object::Real(x as f32)).collect());
            Some(())
        };
        let ending = (at > p.first && segment(&ops[at])).then_some(at);
        let after = (at < p.last && at + 1 >= p.first && segment(&ops[at + 1])).then_some(at + 1);
        for k in [ending, after].into_iter().flatten() {
            whole(ops, k).ok_or_else(|| Error::Invalid("a curve without a start".into()))?;
        }
        let point = end(&ops[at]).ok_or_else(|| Error::Invalid("a node without numbers".into()))?;
        let set = |op: &mut Operation, k: usize, v: [f64; 2]| {
            op.operands[k] = Object::Real(v[0] as f32);
            op.operands[k + 1] = Object::Real(v[1] as f32);
        };
        let start_of = |ops: &Vec<Operation>, k: usize| end(&ops[k - 1]);
        let lerp = |a: [f64; 2], b: [f64; 2], t: f64| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
        match kind {
            NodeKind::Straight => {
                for k in [ending, after].into_iter().flatten() {
                    if ops[k].operator == "c" {
                        let e = end(&ops[k]).unwrap_or(point);
                        ops[k] =
                            Operation::new("l", vec![Object::Real(e[0] as f32), Object::Real(e[1] as f32)]);
                    }
                }
            }
            NodeKind::Corner => {
                if let Some(k) = ending.filter(|k| ops[*k].operator == "c") {
                    set(&mut ops[k], 2, point);
                }
                if let Some(k) = after.filter(|k| ops[*k].operator == "c") {
                    set(&mut ops[k], 0, point);
                }
            }
            NodeKind::Smooth => {
                // The neighbours of the node, and where its handles are now.
                let before = ending.and_then(|k| start_of(ops, k));
                let next = after.and_then(|k| end(&ops[k]));
                let handle_in = ending.and_then(|k| {
                    (ops[k].operator == "c")
                        .then(|| nums(&ops[k].operands).map(|v| [v[2], v[3]]))
                        .flatten()
                });
                let handle_out = after.and_then(|k| {
                    (ops[k].operator == "c")
                        .then(|| nums(&ops[k].operands).map(|v| [v[0], v[1]]))
                        .flatten()
                });
                let len = |a: [f64; 2], b: [f64; 2]| ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
                let unit = |v: [f64; 2]| {
                    let l = (v[0] * v[0] + v[1] * v[1]).sqrt();
                    (l > 1e-9).then(|| [v[0] / l, v[1] / l])
                };
                // The direction through the node: from the handle (or the
                // node) before it to the one after it.
                let from = handle_in.filter(|h| len(*h, point) > 1e-6).or(before);
                let to = handle_out.filter(|h| len(*h, point) > 1e-6).or(next);
                let dir = match (from, to) {
                    (Some(a), Some(b)) => unit([b[0] - a[0], b[1] - a[1]]),
                    (Some(a), None) => unit([point[0] - a[0], point[1] - a[1]]),
                    (None, Some(b)) => unit([b[0] - point[0], b[1] - point[1]]),
                    (None, None) => None,
                }
                .ok_or_else(|| Error::Invalid("the node has no segments to make smooth".into()))?;
                if let (Some(k), Some(b)) = (ending, before) {
                    let reach = handle_in
                        .map(|h| len(h, point))
                        .filter(|l| *l > 1e-6)
                        .unwrap_or(len(b, point) / 3.0);
                    let h = [point[0] - dir[0] * reach, point[1] - dir[1] * reach];
                    if ops[k].operator == "c" {
                        set(&mut ops[k], 2, h);
                    } else {
                        let c1 = lerp(b, point, 1.0 / 3.0);
                        ops[k] = Operation::new(
                            "c",
                            [c1[0], c1[1], h[0], h[1], point[0], point[1]]
                                .into_iter()
                                .map(|x| Object::Real(x as f32))
                                .collect(),
                        );
                    }
                }
                if let (Some(k), Some(e)) = (after, next) {
                    let reach = handle_out
                        .map(|h| len(point, h))
                        .filter(|l| *l > 1e-6)
                        .unwrap_or(len(point, e) / 3.0);
                    let h = [point[0] + dir[0] * reach, point[1] + dir[1] * reach];
                    if ops[k].operator == "c" {
                        set(&mut ops[k], 0, h);
                    } else {
                        let c2 = lerp(point, e, 2.0 / 3.0);
                        ops[k] = Operation::new(
                            "c",
                            [h[0], h[1], c2[0], c2[1], e[0], e[1]]
                                .into_iter()
                                .map(|x| Object::Real(x as f32))
                                .collect(),
                        );
                    }
                }
            }
        }
        self.write_page_content(page, &content)
    }

    /// Draw `shape` through `points` (PDF user space of the page) on page
    /// `page`, stroked in `colour` `width` points wide, in the page content.
    /// One undo step.
    pub fn add_three_point_shape(
        &mut self,
        page: usize,
        shape: ThreePoint,
        points: [[f32; 2]; 3],
        colour: [f32; 3],
        width: f32,
    ) -> Result<()> {
        let page_id = self.page_id(page)?;
        let mut ops = vec![
            Operation::new("q", Vec::new()),
            Operation::new("RG", colour.iter().map(|v| Object::Real(*v)).collect()),
            Operation::new("w", vec![Object::Real(width.max(0.0))]),
        ];
        ops.extend(three_point_ops(shape, points)?);
        ops.push(Operation::new("S", Vec::new()));
        ops.push(Operation::new("Q", Vec::new()));
        let data = Content { operations: ops }
            .encode()
            .map_err(|e| Error::Invalid(format!("the shape cannot be written: {e}")))?;
        self.begin_edit();
        self.append_page_content(page_id, data, None)?;
        self.commit()
    }

    /// Change the colours or the line width of path `path` of page `page`
    /// (`None`: as it is).
    pub fn set_path_style(
        &mut self,
        page: usize,
        path: usize,
        stroke: Option<[f32; 3]>,
        fill: Option<[f32; 3]>,
        width: Option<f32>,
    ) -> Result<()> {
        let (content, base) = self.page_content_ops(page)?;
        let paths = find_paths(&content, base);
        let p = paths
            .get(path)
            .ok_or_else(|| Error::Invalid("no such path".into()))?;
        let mut style = Vec::new();
        let rgb = |c: [f32; 3]| c.iter().map(|v| Object::Real(*v)).collect::<Vec<_>>();
        if let Some(c) = stroke {
            style.push(Operation::new("RG", rgb(c)));
        }
        if let Some(c) = fill {
            style.push(Operation::new("rg", rgb(c)));
        }
        if let Some(w) = width {
            style.push(Operation::new("w", vec![Object::Real(w.max(0.0))]));
        }
        if style.is_empty() {
            return Ok(());
        }
        // The path drawn inside its own graphics state with the new style.
        let mut ops = content.operations;
        let tail = ops.split_off(p.last + 1);
        let path_ops = ops.split_off(p.first);
        ops.push(Operation::new("q", Vec::new()));
        ops.extend(style);
        ops.extend(path_ops);
        ops.push(Operation::new("Q", Vec::new()));
        ops.extend(tail);
        self.write_page_content(page, &Content { operations: ops })
    }

    /// Delete path `path` of page `page`.
    pub fn delete_path(&mut self, page: usize, path: usize) -> Result<()> {
        let (content, base) = self.page_content_ops(page)?;
        let paths = find_paths(&content, base);
        let p = paths
            .get(path)
            .ok_or_else(|| Error::Invalid("no such path".into()))?;
        let mut ops = content.operations;
        ops.drain(p.first..=p.last);
        self.write_page_content(page, &Content { operations: ops })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A page drawing `content`, and the operations of its content after
    /// converting node `node` of its first path to `kind`.
    fn converted(content: &str, node: usize, kind: NodeKind) -> Vec<String> {
        let mut b = crate::testdocs::Builder::new();
        b.page(
            (200.0, 200.0),
            content.as_bytes().to_vec(),
            Dictionary::new(),
            vec![],
        );
        let mut doc = Document::from_bytes(b.finish(), None, None).unwrap();
        doc.convert_path_node(0, 0, node, kind).unwrap();
        let (c, _) = doc.page_content_ops(0).unwrap();
        c.operations
            .iter()
            .map(|o| {
                let v: Vec<String> = nums(&o.operands)
                    .unwrap_or_default()
                    .iter()
                    .map(|x| format!("{}", (x * 100.0).round() / 100.0))
                    .collect();
                format!("{} {}", v.join(" "), o.operator).trim().to_string()
            })
            .collect()
    }

    #[test]
    fn a_corner_between_lines_made_smooth() {
        // 0,0 → 90,0 → 90,90: the middle node.
        let ops = converted("0 0 m 90 0 l 90 90 l S", 1, NodeKind::Smooth);
        assert_eq!(
            ops,
            vec![
                "0 0 m",
                "30 0 68.79 -21.21 90 0 c",
                "111.21 21.21 90 60 90 90 c",
                "S"
            ],
        );
    }

    #[test]
    fn curves_made_straight_and_corner() {
        let curve = "0 0 m 0 50 40 60 50 50 c 60 40 100 50 100 0 c S";
        // The handles of the middle node drawn into it.
        assert_eq!(
            converted(curve, 2, NodeKind::Corner),
            vec!["0 0 m", "0 50 50 50 50 50 c", "50 50 100 50 100 0 c", "S"],
        );
        // Its handle on the next curve stands for it as well.
        assert_eq!(
            converted(curve, 3, NodeKind::Straight),
            vec!["0 0 m", "50 50 l", "100 0 l", "S"],
        );
        // `v` written whole first (its first handle on the node: none);
        // the handles on the line from the node before to the node after,
        // a third of each segment long.
        assert_eq!(
            converted("0 0 m 50 50 l 80 50 100 0 v S", 1, NodeKind::Smooth),
            vec![
                "0 0 m",
                "16.67 16.67 26.43 50 50 50 c",
                "73.57 50 80 50 100 0 c",
                "S"
            ],
        );
    }

    #[test]
    fn shapes_through_three_points() {
        let mut b = crate::testdocs::Builder::new();
        b.page((300.0, 300.0), Vec::new(), Dictionary::new(), vec![]);
        let mut doc = Document::from_bytes(b.finish(), None, None).unwrap();
        let near = |a: [f32; 4], b: [f32; 4]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 0.6);
        // The circle through three points of the circle of centre 150,150 and radius 50.
        doc.add_three_point_shape(
            0,
            ThreePoint::Circle,
            [[100.0, 150.0], [150.0, 200.0], [200.0, 150.0]],
            [1.0, 0.0, 0.0],
            2.0,
        )
        .unwrap();
        let paths = doc.page_paths(0).unwrap();
        assert_eq!(paths.len(), 1);
        assert!(paths[0].stroked && !paths[0].filled);
        assert_eq!(paths[0].stroke, Some([1.0, 0.0, 0.0]));
        // The nodes of four quarters: the handles reach out to the box.
        assert!(
            near(paths[0].bounds, [100.0, 100.0, 200.0, 200.0]),
            "{:?}",
            paths[0].bounds
        );
        // The upper half: from the left through the top to the right.
        doc.add_three_point_shape(
            0,
            ThreePoint::Arc,
            [[100.0, 150.0], [150.0, 200.0], [200.0, 150.0]],
            [0.0; 3],
            1.0,
        )
        .unwrap();
        let arc = &doc.page_paths(0).unwrap()[1];
        assert!(near(arc.bounds, [100.0, 150.0, 200.0, 200.0]), "{:?}", arc.bounds);
        assert_eq!(arc.nodes.len(), 1 + 2 * 3, "two quarters");
        // The lower half when the second point is below.
        doc.add_three_point_shape(
            0,
            ThreePoint::Arc,
            [[100.0, 150.0], [150.0, 100.0], [200.0, 150.0]],
            [0.0; 3],
            1.0,
        )
        .unwrap();
        let arc = &doc.page_paths(0).unwrap()[2];
        assert!(near(arc.bounds, [100.0, 100.0, 200.0, 150.0]), "{:?}", arc.bounds);
        // A rectangle turned: a side from 100,100 to 130,140, as high as the
        // third point is from it (12).
        doc.add_three_point_shape(
            0,
            ThreePoint::Rect,
            [[100.0, 100.0], [130.0, 140.0], [100.0, 120.0]],
            [0.0; 3],
            1.0,
        )
        .unwrap();
        let rect = &doc.page_paths(0).unwrap()[3];
        let corners: Vec<[f32; 2]> = rect
            .nodes
            .iter()
            .map(|n| [(n.at[0] * 10.0).round() / 10.0, (n.at[1] * 10.0).round() / 10.0])
            .collect();
        assert_eq!(
            corners,
            vec![[100.0, 100.0], [130.0, 140.0], [120.4, 147.2], [90.4, 107.2]]
        );
        // Three points on one line make nothing; one undo step a shape.
        assert!(
            doc.add_three_point_shape(
                0,
                ThreePoint::Circle,
                [[0.0, 0.0], [10.0, 10.0], [20.0, 20.0]],
                [0.0; 3],
                1.0
            )
            .is_err()
        );
        doc.undo().unwrap();
        assert_eq!(doc.page_paths(0).unwrap().len(), 3);
    }

    #[test]
    fn rectangles_keep_their_corners() {
        let mut b = crate::testdocs::Builder::new();
        b.page(
            (200.0, 200.0),
            b"10 10 50 50 re f".to_vec(),
            Dictionary::new(),
            vec![],
        );
        let mut doc = Document::from_bytes(b.finish(), None, None).unwrap();
        assert!(doc.convert_path_node(0, 0, 1, NodeKind::Smooth).is_err());
    }

    #[test]
    fn paths_with_their_nodes_colours_and_matrix() {
        let data = b"q 2 0 0 2 10 10 cm 1 0 0 RG 3 w 0 0 m 10 0 l 10 10 l S Q 0 0 1 rg 50 50 20 10 re f 0 0 m 5 5 l W n BT /F1 12 Tf 0 0 Td (x) Tj ET";
        let content = Content::decode(data).unwrap();
        let paths = find_paths(&content, Affine::IDENTITY);
        assert_eq!(paths.len(), 2, "the clipping path is not offered");
        let a = &paths[0];
        assert!(a.stroked && !a.filled);
        assert_eq!(a.stroke, Some([1.0, 0.0, 0.0]));
        assert_eq!(a.width, 3.0);
        assert_eq!(a.nodes.len(), 3);
        assert_eq!(a.nodes[1].at, [30.0, 10.0], "drawn with its matrix");
        let b = &paths[1];
        assert!(b.filled && !b.stroked);
        assert_eq!(b.fill, Some([0.0, 0.0, 1.0]));
        assert_eq!(b.nodes.len(), 2);
        assert_eq!(b.nodes[1].at, [70.0, 60.0]);
        assert_eq!(b.bounds, [50.0, 50.0, 70.0, 60.0]);
    }
}

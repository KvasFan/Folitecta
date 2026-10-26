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

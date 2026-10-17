//! Arranging comments on a page: aligning them, spreading them evenly,
//! giving them the size of the first one, and the order in which they are
//! stacked. Positions are worked out as the page is shown (a turned page
//! included); every arrangement is one undo step.

use lopdf::{Object, ObjectId};

use crate::document::Document;
use crate::error::{Error, Result};
use crate::marks::pdf_rect_to_display;
use crate::textops::display_rect_to_pdf;
use crate::util::rect_of;

/// The edge or middle line comments are aligned on, as the page is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Left,
    Center,
    Right,
    Top,
    Middle,
    Bottom,
}

/// What comments are aligned with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignTo {
    /// The box around all of them.
    Selection,
    /// The page.
    Page,
    /// The page, the comments moved together (their box aligned).
    PageAsGroup,
    /// The first comment chosen (the key object).
    First,
}

/// Across the page (left to right) or down it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    Across,
    Down,
}

/// What is spread evenly: the start edges (left or top), the middles, the
/// end edges, or the space between the comments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpreadBy {
    Start,
    Middle,
    End,
    Space,
}

/// The size given to the comments: that of the first one chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SameSize {
    Width,
    Height,
    Both,
}

/// Where comments go in the order they are drawn (later ones on top).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stack {
    /// On top of every other comment.
    Front,
    /// Under every other comment.
    Back,
    /// One step up.
    Forward,
    /// One step down.
    Backward,
}

/// Rectangles as the page shows them: `[x0, y0, x1, y1]`, y down.
type Shown = [f32; 4];

/// A measure of a rectangle along an axis (where it starts, how long it is).
type Measure = fn(&Shown) -> f32;

fn left(r: &Shown) -> f32 {
    r[0]
}

fn top(r: &Shown) -> f32 {
    r[1]
}

fn width(r: &Shown) -> f32 {
    r[2] - r[0]
}

fn height(r: &Shown) -> f32 {
    r[3] - r[1]
}

/// Move a rectangle by (dx, dy).
fn moved(r: Shown, dx: f32, dy: f32) -> Shown {
    [r[0] + dx, r[1] + dy, r[2] + dx, r[3] + dy]
}

impl Document {
    /// The annotation objects and their rectangles as the page shows them.
    fn shown_rects(&self, page: usize, indices: &[usize]) -> Result<Vec<(ObjectId, Shown)>> {
        let size = self.page_size(page)?;
        let mut out = Vec::with_capacity(indices.len());
        for &index in indices {
            let (_, aid) = self.annotation_ref(page, index)?;
            let aid = aid.ok_or_else(|| Error::Invalid("inline comments cannot be arranged".into()))?;
            let rect = self
                .inner
                .get_dictionary(aid)?
                .get(b"Rect")
                .ok()
                .and_then(|r| rect_of(&self.inner, r))
                .ok_or_else(|| Error::Invalid("a comment has no rectangle".into()))?;
            out.push((aid, pdf_rect_to_display(&size, crate::geom::normalize(rect))));
        }
        Ok(out)
    }

    /// Give the comments their new rectangles (as the page shows them), in
    /// one undo step.
    fn place_shown(&mut self, page: usize, placed: &[(ObjectId, Shown)]) -> Result<()> {
        let size = self.page_size(page)?;
        self.begin_edit();
        for (aid, r) in placed {
            self.reshape_annotation(*aid, crate::geom::normalize(display_rect_to_pdf(&size, *r)))?;
        }
        self.commit()
    }

    /// Align comments of a page on an edge or a middle line.
    pub fn align_annotations(
        &mut self,
        page: usize,
        indices: &[usize],
        edge: Edge,
        to: AlignTo,
    ) -> Result<()> {
        let items = self.shown_rects(page, indices)?;
        let least = if matches!(to, AlignTo::Page | AlignTo::PageAsGroup) {
            1
        } else {
            2
        };
        if items.len() < least {
            return Err(Error::Invalid(format!(
                "choose at least {least} comments to align"
            )));
        }
        let size = self.page_size(page)?;
        let page_box: Shown = [0.0, 0.0, size.width, size.height];
        let around = items
            .iter()
            .fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |a, (_, r)| {
                [a[0].min(r[0]), a[1].min(r[1]), a[2].max(r[2]), a[3].max(r[3])]
            });
        // The box the comments are aligned in.
        let frame: Shown = match to {
            AlignTo::Page | AlignTo::PageAsGroup => page_box,
            AlignTo::First => items[0].1,
            AlignTo::Selection => around,
        };
        // How far a box moves to be aligned in the frame.
        let shift = |r: &Shown| match edge {
            Edge::Left => (frame[0] - r[0], 0.0),
            Edge::Right => (frame[2] - r[2], 0.0),
            Edge::Center => ((frame[0] + frame[2]) / 2.0 - (r[0] + r[2]) / 2.0, 0.0),
            Edge::Top => (0.0, frame[1] - r[1]),
            Edge::Bottom => (0.0, frame[3] - r[3]),
            Edge::Middle => (0.0, (frame[1] + frame[3]) / 2.0 - (r[1] + r[3]) / 2.0),
        };
        // As a group, every comment moves as far as the box around them.
        let together = (to == AlignTo::PageAsGroup).then(|| shift(&around));
        let placed: Vec<(ObjectId, Shown)> = items
            .into_iter()
            .map(|(aid, r)| {
                let (dx, dy) = together.unwrap_or_else(|| shift(&r));
                (aid, moved(r, dx, dy))
            })
            .collect();
        self.place_shown(page, &placed)
    }

    /// Spread comments evenly across or down the page: between the first
    /// and the last of them, or (`on_page`) from one side of the page to the
    /// other.
    pub fn distribute_annotations(
        &mut self,
        page: usize,
        indices: &[usize],
        axis: Axis,
        by: SpreadBy,
        on_page: bool,
    ) -> Result<()> {
        let mut items = self.shown_rects(page, indices)?;
        let least = if on_page { 2 } else { 3 };
        if items.len() < least {
            return Err(Error::Invalid(format!(
                "choose at least {least} comments to distribute"
            )));
        }
        let size = self.page_size(page)?;
        // Position and length of a rectangle along the axis.
        let (start, len): (Measure, Measure) = match axis {
            Axis::Across => (left, width),
            Axis::Down => (top, height),
        };
        items.sort_by(|a, b| start(&a.1).total_cmp(&start(&b.1)));
        let n = items.len();
        let (first, last) = (items[0].1, items[n - 1].1);
        let extent = match axis {
            Axis::Across => size.width,
            Axis::Down => size.height,
        };
        // Where the first comment starts and the last one ends.
        let (lo, hi) = if on_page {
            (0.0, extent)
        } else {
            (start(&first), start(&last) + len(&last))
        };
        let mut starts = Vec::with_capacity(n);
        match by {
            SpreadBy::Space => {
                let total: f32 = items.iter().map(|(_, r)| len(r)).sum();
                let gap = (hi - lo - total) / (n - 1) as f32;
                let mut at = lo;
                for (_, r) in &items {
                    starts.push(at);
                    at += len(r) + gap;
                }
            }
            SpreadBy::Start | SpreadBy::Middle | SpreadBy::End => {
                // The point of each comment that is spread: its start, middle or end.
                let anchor = |r: &Shown| match by {
                    SpreadBy::Start => 0.0,
                    SpreadBy::Middle => len(r) / 2.0,
                    _ => len(r),
                };
                let a = lo + anchor(&first);
                let b = hi - len(&last) + anchor(&last);
                for (i, (_, r)) in items.iter().enumerate() {
                    let point = a + (b - a) * i as f32 / (n - 1) as f32;
                    starts.push(point - anchor(r));
                }
            }
        }
        let placed: Vec<(ObjectId, Shown)> = items
            .iter()
            .zip(starts)
            .map(|((aid, r), s)| {
                let d = s - start(r);
                let to = match axis {
                    Axis::Across => moved(*r, d, 0.0),
                    Axis::Down => moved(*r, 0.0, d),
                };
                (*aid, to)
            })
            .collect();
        self.place_shown(page, &placed)
    }

    /// Give comments the width, the height or both of the first one; each
    /// keeps its top-left corner.
    pub fn size_annotations(&mut self, page: usize, indices: &[usize], what: SameSize) -> Result<()> {
        let items = self.shown_rects(page, indices)?;
        if items.len() < 2 {
            return Err(Error::Invalid("choose at least 2 comments".into()));
        }
        let key = items[0].1;
        let placed: Vec<(ObjectId, Shown)> = items
            .into_iter()
            .map(|(aid, r)| {
                let w = if what == SameSize::Height {
                    width(&r)
                } else {
                    width(&key)
                };
                let h = if what == SameSize::Width {
                    height(&r)
                } else {
                    height(&key)
                };
                (aid, [r[0], r[1], r[0] + w, r[1] + h])
            })
            .collect();
        self.place_shown(page, &placed)
    }

    /// Move comments up or down the order the page draws them in. Returns
    /// their new indices, in the order of `indices`.
    pub fn stack_annotations(&mut self, page: usize, indices: &[usize], how: Stack) -> Result<Vec<usize>> {
        let page_id = self.page_id(page)?;
        let (arr, holder) = match self.inner.get_dictionary(page_id)?.get(b"Annots") {
            Ok(Object::Array(a)) => (a.clone(), None),
            Ok(Object::Reference(r)) => (self.inner.get_object(*r)?.as_array()?.clone(), Some(*r)),
            _ => return Err(Error::Invalid("the page has no comments".into())),
        };
        if indices.is_empty() || indices.iter().any(|i| *i >= arr.len()) {
            return Err(Error::Invalid("choose the comments to move".into()));
        }
        // Positions of the moved entries, tracked by their objects.
        let chosen: Vec<Object> = indices.iter().map(|i| arr[*i].clone()).collect();
        let mut order: Vec<(Object, bool)> = arr
            .iter()
            .enumerate()
            .map(|(i, o)| (o.clone(), indices.contains(&i)))
            .collect();
        match how {
            Stack::Front | Stack::Back => {
                let (picked, rest): (Vec<_>, Vec<_>) = order.into_iter().partition(|(_, p)| *p);
                order = if how == Stack::Front {
                    rest.into_iter().chain(picked).collect()
                } else {
                    picked.into_iter().chain(rest).collect()
                };
            }
            Stack::Forward => {
                for i in (0..order.len().saturating_sub(1)).rev() {
                    if order[i].1 && !order[i + 1].1 {
                        order.swap(i, i + 1);
                    }
                }
            }
            Stack::Backward => {
                for i in 1..order.len() {
                    if order[i].1 && !order[i - 1].1 {
                        order.swap(i, i - 1);
                    }
                }
            }
        }
        let new_arr: Vec<Object> = order.into_iter().map(|(o, _)| o).collect();
        let new_indices: Vec<usize> = chosen
            .iter()
            .filter_map(|c| new_arr.iter().position(|o| o == c))
            .collect();
        if new_arr == arr {
            return Ok(new_indices);
        }
        self.begin_edit();
        match holder {
            Some(r) => *self.inner.get_object_mut(r)? = Object::Array(new_arr),
            None => self
                .inner
                .get_dictionary_mut(page_id)?
                .set("Annots", Object::Array(new_arr)),
        }
        self.commit()?;
        Ok(new_indices)
    }
}

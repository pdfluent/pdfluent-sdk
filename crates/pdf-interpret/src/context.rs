use crate::cache::{Cache, CacheKey};
use crate::color::ColorSpace;
use crate::convert::convert_transform;
use crate::font::{Font, StandardFont};
use crate::interpret::state::{ClipType, State, TextStateFont};
use crate::ocg::OcgState;
use crate::util::{BezPathExt, Float64Ext};
use crate::{ClipPath, Device, FillRule, InterpreterSettings, StrokeProps};
use kurbo::{Affine, BezPath, PathEl, Point, Rect, Shape};
use log::warn;
use pdf_syntax::content::ops::Transform;
use pdf_syntax::object::Dict;
use pdf_syntax::object::Name;
use pdf_syntax::page::Resources;
use pdf_syntax::xref::XRef;
use std::collections::HashMap;

/// How deep a document may nest one interpretation inside another.
///
/// A Form XObject may draw another XObject, a pattern may paint with a pattern,
/// a Type 3 glyph is a content stream of its own. Nothing in a PDF stops any of
/// those from referring to itself, and the interpreter recurses on the Rust
/// stack -- so a self-referencing XObject exhausts it. Reproduced on a 639-byte
/// file: `q /X1 Do Q` inside the XObject named `/X1`, rc=134, "has overflowed
/// its stack". No XFA and no script; it reaches every binding that exports
/// `render_page`.
///
/// 50 because upstream chose 50 (LaurenzV/hayro#1152) and because real documents
/// do not come close: the deepest nesting in the corpus is single digits. The
/// limit exists to convert a crash into a warning and a missing paint, not to
/// judge how baroque a document is allowed to be.
pub(crate) const MAX_NESTED_INTERPRETATION_DEPTH: u32 = 50;

/// A context for interpreting PDF files.
pub struct Context<'a> {
    states: Vec<State<'a>>,
    path: BezPath,
    sub_path_start: Point,
    last_point: Point,
    clip: Option<FillRule>,
    pub(crate) font_cache: HashMap<u128, Option<Font<'a>>>,
    root_transforms: Vec<Affine>,
    bbox: Vec<Rect>,
    pub(crate) settings: InterpreterSettings,
    pub(crate) object_cache: Cache,
    pub(crate) xref: &'a XRef,
    pub(crate) ocg_state: OcgState,
    /// How deep this interpretation already sits inside XObjects, patterns,
    /// soft masks or Type 3 glyph procedures. See MAX_NESTED_INTERPRETATION_DEPTH.
    nesting_depth: u32,
}

impl<'a> Context<'a> {
    /// Create a new context.
    pub fn new(
        initial_transform: Affine,
        bbox: Rect,
        xref: &'a XRef,
        settings: InterpreterSettings,
    ) -> Self {
        let cache = settings.shared_cache.clone().unwrap_or_default();
        let state = State::new(initial_transform);

        Self::new_with(initial_transform, bbox, cache, xref, settings, state, 0)
    }

    pub(crate) fn new_with(
        initial_transform: Affine,
        bbox: Rect,
        cache: Cache,
        xref: &'a XRef,
        settings: InterpreterSettings,
        state: State<'a>,
        nesting_depth: u32,
    ) -> Self {
        let ocg_state = {
            let root_ref = xref.root_id();
            xref.get::<Dict<'_>>(root_ref)
                .map(|catalog| OcgState::from_catalog(&catalog))
                .unwrap_or_default()
        };

        Self {
            nesting_depth,
            states: vec![state],
            settings,
            xref,
            root_transforms: vec![initial_transform],
            last_point: Point::default(),
            sub_path_start: Point::default(),
            clip: None,
            bbox: vec![bbox],
            path: BezPath::new(),
            font_cache: HashMap::new(),
            object_cache: cache,
            ocg_state,
        }
    }

    /// Return the interpreter settings owned by this context.
    pub fn into_settings(self) -> InterpreterSettings {
        self.settings
    }

    pub(crate) fn save_state(&mut self) {
        let Some(cur) = self.states.last().cloned() else {
            warn!("attempted to save state without existing state");
            return;
        };

        self.states.push(cur);
    }

    pub(crate) fn bbox(&self) -> Rect {
        self.bbox.last().copied().unwrap_or_else(|| {
            warn!("failed to get a bbox");

            Rect::new(0.0, 0.0, 1.0, 1.0)
        })
    }

    fn push_bbox(&mut self, bbox: Rect) {
        let new = self.bbox().intersect(bbox);
        self.bbox.push(new);
    }

    pub(crate) fn push_clip_path(
        &mut self,
        clip_path: BezPath,
        fill: FillRule,
        device: &mut impl Device<'a>,
    ) {
        if let Some(clip_rect) = path_as_rect(&clip_path) {
            let cur_bbox = self.bbox();

            // If the clip path is a rect and completely covers the current bbox, don't emit it.
            if cur_bbox
                .min_x()
                .is_nearly_greater_or_equal(clip_rect.min_x())
                && cur_bbox
                    .min_y()
                    .is_nearly_greater_or_equal(clip_rect.min_y())
                && cur_bbox.max_x().is_nearly_less_or_equal(clip_rect.max_x())
                && cur_bbox.max_y().is_nearly_less_or_equal(clip_rect.max_y())
            {
                self.get_mut().clips.push(ClipType::Dummy);
                return;
            }
        }

        let bbox = clip_path.bounding_box();
        device.push_clip_path(&ClipPath {
            path: clip_path,
            fill,
        });
        self.push_bbox(bbox);
        self.get_mut().clips.push(ClipType::Real);
    }

    pub(crate) fn pop_clip_path(&mut self, device: &mut impl Device<'a>) {
        if let Some(ClipType::Real) = self.get_mut().clips.pop() {
            device.pop_clip_path();
            self.pop_bbox();
        }
    }

    fn pop_bbox(&mut self) {
        self.bbox.pop();
    }

    pub(crate) fn push_root_transform(&mut self) {
        self.root_transforms.push(self.get().ctm);
    }

    pub(crate) fn pop_root_transform(&mut self) {
        self.root_transforms.pop();
    }

    pub(crate) fn root_transform(&self) -> Affine {
        self.root_transforms
            .last()
            .copied()
            .unwrap_or(Affine::IDENTITY)
    }

    pub(crate) fn restore_state(&mut self, device: &mut impl Device<'a>) {
        let Some(target_clips) = self
            .states
            .get(self.states.len().saturating_sub(2))
            .map(|s| s.clips.len())
        else {
            warn!("underflowed graphics state");
            return;
        };

        while self.get().clips.len() > target_clips {
            self.pop_clip_path(device);
        }

        // The first state should never be popped.
        if self.states.len() > 1 {
            self.states.pop();
        }

        device.set_soft_mask(
            self.states
                .last()
                .and_then(|l| l.graphics_state.soft_mask.clone()),
        );
    }

    pub(crate) fn path(&self) -> &BezPath {
        &self.path
    }

    pub(crate) fn path_mut(&mut self) -> &mut BezPath {
        &mut self.path
    }

    pub(crate) fn sub_path_start(&self) -> &Point {
        &self.sub_path_start
    }

    pub(crate) fn sub_path_start_mut(&mut self) -> &mut Point {
        &mut self.sub_path_start
    }

    pub(crate) fn last_point(&self) -> &Point {
        &self.last_point
    }

    pub(crate) fn last_point_mut(&mut self) -> &mut Point {
        &mut self.last_point
    }

    pub(crate) fn clip(&self) -> &Option<FillRule> {
        &self.clip
    }

    pub(crate) fn clip_mut(&mut self) -> &mut Option<FillRule> {
        &mut self.clip
    }

    // A `nesting_depth()` accessor belongs here and is deliberately absent: it
    // would have no caller until the parent depth is threaded into the three
    // constructs that build a fresh Context (soft mask, Type 3 glyph, tiling
    // pattern), and an unused accessor kept alive by an allow(dead_code) is the
    // shape these guards exist to refuse. It comes back with that work (#318).

    /// Claim one level of nesting, or refuse.
    ///
    /// Returns false at the limit; the caller must then not interpret. Pair with
    /// `end_nested_interpretation`, which is why this is not a plain `+= 1`: a
    /// page draws many XObjects in sequence, and a counter that only rises would
    /// refuse the fifty-first sibling rather than the fifty-first ancestor.
    pub(crate) fn begin_nested_interpretation(&mut self) -> bool {
        if self.nesting_depth >= MAX_NESTED_INTERPRETATION_DEPTH {
            return false;
        }

        self.nesting_depth += 1;

        true
    }

    pub(crate) fn end_nested_interpretation(&mut self) {
        self.nesting_depth = self.nesting_depth.saturating_sub(1);
    }

    pub(crate) fn get(&self) -> &State<'a> {
        self.states.last().unwrap()
    }

    pub(crate) fn get_mut(&mut self) -> &mut State<'a> {
        self.states.last_mut().unwrap()
    }

    pub(crate) fn pre_concat_transform(&mut self, transform: Transform) {
        self.pre_concat_affine(convert_transform(transform));
    }

    pub(crate) fn pre_concat_affine(&mut self, transform: Affine) {
        self.get_mut().ctm *= transform;
    }

    pub(crate) fn get_color_space(
        &mut self,
        resources: &Resources<'_>,
        name: Name,
    ) -> Option<ColorSpace> {
        let cs_object = resources.get_color_space(name)?;
        self.object_cache
            .get_or_insert_with(cs_object.cache_key(), || {
                ColorSpace::new(
                    cs_object.clone(),
                    &self.object_cache,
                    &self.settings.warning_sink,
                )
            })
    }

    pub(crate) fn stroke_props(&self) -> StrokeProps {
        self.get().graphics_state.stroke_props.clone()
    }

    pub(crate) fn num_states(&self) -> usize {
        self.states.len()
    }

    pub(crate) fn resolve_font(&mut self, font_dict: &Dict<'a>) -> Option<TextStateFont<'a>> {
        let cache_key = font_dict.cache_key();

        if let Some(resolved) = self
            .font_cache
            .entry(cache_key)
            .or_insert_with(|| {
                Font::new(
                    font_dict,
                    &self.settings.font_resolver,
                    &self.settings.cmap_resolver,
                    &self.settings.warning_sink,
                )
            })
            .clone()
        {
            Some(TextStateFont::Font(resolved))
        } else {
            Font::new_standard(StandardFont::Helvetica, &self.settings.font_resolver)
                .map(TextStateFont::Fallback)
        }
    }
}

pub(crate) fn path_as_rect(path: &BezPath) -> Option<Rect> {
    let points = match path.elements() {
        [
            PathEl::MoveTo(first),
            PathEl::LineTo(second),
            PathEl::LineTo(third),
            PathEl::LineTo(fourth),
            PathEl::ClosePath,
        ] => [*first, *second, *third, *fourth, *first],
        [
            PathEl::MoveTo(first),
            PathEl::LineTo(second),
            PathEl::LineTo(third),
            PathEl::LineTo(fourth),
            PathEl::LineTo(last),
        ] if first.x.is_nearly_equal(last.x) && first.y.is_nearly_equal(last.y) => {
            [*first, *second, *third, *fourth, *last]
        }
        _ => return None,
    };

    let mut previous_axis = None;
    for edge in points.windows(2) {
        let same_x = edge[0].x.is_nearly_equal(edge[1].x);
        let same_y = edge[0].y.is_nearly_equal(edge[1].y);
        if same_x == same_y || previous_axis == Some(same_x) {
            return None;
        }
        previous_axis = Some(same_x);
    }

    Some(path.fast_bounding_box())
}

#[cfg(test)]
mod clip_path_shape {
    use super::*;

    /// A bowtie is not a rectangle, however many of its corners are.
    ///
    /// The version this replaces decided by collecting which corners of the
    /// bounding box the path touches. A self-intersecting quadrilateral touches
    /// all four, so it was accepted and a clip shaped like an hourglass became
    /// its bounding box -- content drawn where the document said to hide it.
    /// Ported from LaurenzV/hayro#1338; measured here before and after, the same
    /// path answered `true` and now answers `false`.
    #[test]
    fn a_bowtie_is_not_a_rect() {
        let mut bowtie = BezPath::new();
        bowtie.move_to((0.0, 0.0));
        bowtie.line_to((10.0, 10.0));
        bowtie.line_to((10.0, 0.0));
        bowtie.line_to((0.0, 10.0));
        bowtie.close_path();

        assert!(
            path_as_rect(&bowtie).is_none(),
            "a self-intersecting quadrilateral was accepted as a rectangle, so a \
             clip would cover its bounding box instead of its actual shape"
        );
    }

    /// The other half: the fix must not refuse ordinary rectangles.
    ///
    /// Without this, returning `None` unconditionally would pass the test above
    /// and turn every rectangular clip into a general path -- slower, and a
    /// silent behaviour change nobody asked for.
    #[test]
    fn an_ordinary_rectangle_still_is_one() {
        let mut rect = BezPath::new();
        rect.move_to((0.0, 0.0));
        rect.line_to((10.0, 0.0));
        rect.line_to((10.0, 10.0));
        rect.line_to((0.0, 10.0));
        rect.close_path();

        assert!(
            path_as_rect(&rect).is_some(),
            "a plain rectangle stopped being recognised as one"
        );
    }

    /// The unclosed arm: `MoveTo + 4 LineTo` is accepted only when the last
    /// point returns to the first.
    ///
    /// This is a separate acceptance path from the `ClosePath` form, and it is
    /// also a tightening -- the version it replaces took that shape whether or
    /// not it closed. Without a test, deleting the arm's guard or inverting it
    /// changes nothing that goes red, and it is the arm most likely to be
    /// "simplified" later by someone who sees two arms doing the same thing.
    #[test]
    fn four_lines_that_do_not_return_to_the_start_are_not_a_rect() {
        let mut open = BezPath::new();
        open.move_to((0.0, 0.0));
        open.line_to((10.0, 0.0));
        open.line_to((10.0, 10.0));
        open.line_to((0.0, 10.0));
        open.line_to((0.0, 5.0)); // back down the left edge, but not to the start

        assert!(
            path_as_rect(&open).is_none(),
            "an unclosed four-line path was accepted as a rectangle, so a clip \
             would cover the closed shape the path never drew"
        );
    }

    /// And the same arm must still accept the shape it exists for.
    ///
    /// Paired with the test above so that neither "always None" nor "always
    /// Some" passes both.
    #[test]
    fn four_lines_that_do_return_to_the_start_are_a_rect() {
        let mut closed = BezPath::new();
        closed.move_to((0.0, 0.0));
        closed.line_to((10.0, 0.0));
        closed.line_to((10.0, 10.0));
        closed.line_to((0.0, 10.0));
        closed.line_to((0.0, 0.0));

        assert!(
            path_as_rect(&closed).is_some(),
            "a four-line rectangle that returns to its start stopped being one"
        );
    }
}

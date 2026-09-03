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
    // One MoveTo, three LineTo, one ClosePath
    if path.elements().len() != 5 {
        return None;
    }

    let bbox = path.fast_bounding_box();
    let (min_x, min_y, max_x, max_y) = (bbox.min_x(), bbox.min_y(), bbox.max_x(), bbox.max_y());
    let mut corners = [false; 4];

    let mut check_point = |p: Point| {
        corners[0] |= p.x.is_nearly_equal(min_x) && p.y.is_nearly_equal(min_y);
        corners[1] |= p.x.is_nearly_equal(min_x) && p.y.is_nearly_equal(max_y);
        corners[2] |= p.x.is_nearly_equal(max_x) && p.y.is_nearly_equal(min_y);
        corners[3] |= p.x.is_nearly_equal(max_x) && p.y.is_nearly_equal(max_y);
    };

    for (idx, el) in path.elements().iter().enumerate() {
        match el {
            PathEl::MoveTo(p) => {
                if idx != 0 {
                    return None;
                }

                check_point(*p);
            }
            PathEl::LineTo(l) => check_point(*l),
            PathEl::QuadTo(_, _) => return None,
            PathEl::CurveTo(_, _, _) => return None,
            PathEl::ClosePath => {}
        }
    }

    if corners[0] && corners[1] && corners[2] && corners[3] {
        Some(bbox)
    } else {
        None
    }
}

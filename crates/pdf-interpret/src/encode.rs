//! Encoding shading patterns for easy sampling.

use crate::color::{AlphaColor, ColorComponents, ColorSpace};
use crate::interpret::state::ActiveTransferFunction;
use crate::pattern::ShadingPattern;
use crate::shading::{ShadingFunction, ShadingType, Triangle};
use kurbo::{Affine, Point};
use rustc_hash::FxHashMap;
use smallvec::{ToSmallVec, smallvec};

/// A shading pattern that was encoded so it can be sampled.
#[derive(Debug)]
pub struct EncodedShadingPattern {
    /// The base transform of the shading pattern.
    pub base_transform: Affine,
    pub(crate) color_space: ColorSpace,
    pub(crate) background_color: AlphaColor,
    pub(crate) shading_type: EncodedShadingType,
    pub(crate) opacity: f32,
    pub(crate) transfer_function: Option<ActiveTransferFunction>,
}

impl EncodedShadingPattern {
    /// Sample the shading at the given position.
    #[inline]
    pub fn sample(&self, pos: Point) -> [f32; 4] {
        self.shading_type
            .eval(pos, self.background_color, &self.color_space)
            .map(|v| {
                let mut components = v.components();
                components[3] *= self.opacity;

                if let Some(tf) = &self.transfer_function {
                    return tf.apply(&AlphaColor::new(components)).components();
                }

                components
            })
            .unwrap_or([0.0, 0.0, 0.0, 0.0])
    }
}

impl ShadingPattern {
    /// Encode the shading pattern.
    pub fn encode(&self) -> EncodedShadingPattern {
        let base_transform;

        let shading_type = match self.shading.shading_type.as_ref() {
            ShadingType::FunctionBased {
                domain,
                matrix,
                function,
            } => {
                base_transform = (self.matrix * *matrix).inverse();
                encode_function_shading(domain, function)
            }
            ShadingType::RadialAxial {
                coords,
                domain,
                function,
                extend,
                axial,
            } => {
                let (encoded, initial_transform) =
                    encode_axial_shading(*coords, *domain, function, *extend, *axial);

                base_transform = initial_transform * self.matrix.inverse();

                encoded
            }
            ShadingType::TriangleMesh {
                triangles,
                function,
            } => {
                let full_transform = self.matrix;
                let samples = sample_triangles(triangles, full_transform);

                base_transform = Affine::IDENTITY;

                EncodedShadingType::Sampled {
                    samples,
                    function: function.clone(),
                }
            }
            ShadingType::CoonsPatchMesh { patches, function } => {
                let mut triangles = vec![];
                for patch in patches {
                    patch.to_triangles(&mut triangles);
                }

                let full_transform = self.matrix;
                let samples = sample_triangles(&triangles, full_transform);

                base_transform = Affine::IDENTITY;

                EncodedShadingType::Sampled {
                    samples,
                    function: function.clone(),
                }
            }
            ShadingType::TensorProductPatchMesh { patches, function } => {
                let mut triangles = vec![];
                for patch in patches {
                    patch.to_triangles(&mut triangles);
                }

                let full_transform = self.matrix;
                let samples = sample_triangles(&triangles, full_transform);

                base_transform = Affine::IDENTITY;

                EncodedShadingType::Sampled {
                    samples,
                    function: function.clone(),
                }
            }
            ShadingType::Dummy => {
                base_transform = Affine::IDENTITY;

                EncodedShadingType::Dummy
            }
        };

        let color_space = self.shading.color_space.clone();

        let background_color = self
            .shading
            .background
            .as_ref()
            .map(|b| color_space.to_rgba(b, 1.0, false))
            .unwrap_or(AlphaColor::TRANSPARENT);

        EncodedShadingPattern {
            color_space,
            background_color,
            shading_type,
            base_transform,
            opacity: self.opacity,
            transfer_function: self.transfer_function.clone(),
        }
    }
}

fn encode_axial_shading(
    coords: [f32; 6],
    domain: [f32; 2],
    function: &ShadingFunction,
    extend: [bool; 2],
    is_axial: bool,
) -> (EncodedShadingType, Affine) {
    let initial_transform;

    let params = if is_axial {
        let [x_0, y_0, x_1, y_1, _, _] = coords;

        initial_transform = ts_from_line_to_line(
            Point::new(x_0 as f64, y_0 as f64),
            Point::new(x_1 as f64, y_1 as f64),
            Point::ZERO,
            Point::new(1.0, 0.0),
        );

        RadialAxialParams::Axial
    } else {
        let [x_0, y_0, r0, x_1, y_1, r_1] = coords;

        initial_transform = Affine::translate((-x_0 as f64, -y_0 as f64));
        let new_x1 = x_1 - x_0;
        let new_y1 = y_1 - y_0;

        let p1 = Point::new(new_x1 as f64, new_y1 as f64);
        let r = Point::new(r0 as f64, r_1 as f64);

        RadialAxialParams::Radial { p1, r }
    };

    (
        EncodedShadingType::RadialAxial {
            function: function.clone(),
            params,
            domain,
            extend,
        },
        initial_transform,
    )
}

fn sample_triangles(
    triangles: &[Triangle],
    transform: Affine,
) -> FxHashMap<(i32, i32), ColorComponents> {
    let mut map = FxHashMap::default();

    for t in triangles {
        let t = {
            let p0 = transform * t.p0.point;
            let p1 = transform * t.p1.point;
            let p2 = transform * t.p2.point;

            let mut v0 = t.p0.clone();
            v0.point = p0;
            let mut v1 = t.p1.clone();
            v1.point = p1;
            let mut v2 = t.p2.clone();
            v2.point = p2;

            Triangle::new(v0, v1, v2)
        };

        let bbox = t.bounding_box();

        // Use i32 keys so that negative coordinates (e.g. patterns that start
        // left/above the page origin) and large coordinate values (> 65535)
        // are represented correctly without wrapping or saturation.
        for y in (bbox.y0.floor() as i32)..(bbox.y1.ceil() as i32) {
            for x in (bbox.x0.floor() as i32)..(bbox.x1.ceil() as i32) {
                let point = Point::new(x as f64, y as f64);
                if t.contains_point(point) {
                    map.insert((x, y), t.interpolate(point));
                }
            }
        }
    }

    map
}

fn encode_function_shading(domain: &[f32; 4], function: &ShadingFunction) -> EncodedShadingType {
    let domain = kurbo::Rect::new(
        domain[0] as f64,
        domain[2] as f64,
        domain[1] as f64,
        domain[3] as f64,
    );

    EncodedShadingType::FunctionBased {
        domain,
        function: function.clone(),
    }
}

#[derive(Debug)]
pub(crate) enum RadialAxialParams {
    Axial,
    Radial { p1: Point, r: Point },
}

#[derive(Debug)]
pub(crate) enum EncodedShadingType {
    FunctionBased {
        domain: kurbo::Rect,
        function: ShadingFunction,
    },
    RadialAxial {
        function: ShadingFunction,
        params: RadialAxialParams,
        domain: [f32; 2],
        extend: [bool; 2],
    },
    Sampled {
        samples: FxHashMap<(i32, i32), ColorComponents>,
        function: Option<ShadingFunction>,
    },
    Dummy,
}

impl EncodedShadingType {
    pub(crate) fn eval(
        &self,
        pos: Point,
        bg_color: AlphaColor,
        color_space: &ColorSpace,
    ) -> Option<AlphaColor> {
        match self {
            Self::FunctionBased { domain, function } => {
                if !domain.contains(pos) {
                    Some(bg_color)
                } else {
                    let out = function.eval(&smallvec![pos.x as f32, pos.y as f32])?;
                    // TODO: Clamp out-of-range values.
                    Some(color_space.to_rgba(&out, 1.0, false))
                }
            }
            Self::RadialAxial {
                function,
                params,
                domain,
                extend,
            } => {
                let (t0, t1) = (domain[0], domain[1]);

                let mut t = match params {
                    RadialAxialParams::Axial => pos.x as f32,
                    RadialAxialParams::Radial { p1, r } => {
                        radial_pos(&pos, p1, *r, extend[0], extend[1]).unwrap_or(f32::MIN)
                    }
                };

                if t == f32::MIN {
                    return Some(bg_color);
                }

                if t < 0.0 {
                    if extend[0] {
                        t = 0.0;
                    } else {
                        return Some(bg_color);
                    }
                } else if t > 1.0 {
                    if extend[1] {
                        t = 1.0;
                    } else {
                        return Some(bg_color);
                    }
                }

                let t = t0 + (t1 - t0) * t;

                let val = function.eval(&smallvec![t])?;

                Some(color_space.to_rgba(&val, 1.0, false))
            }
            Self::Sampled { samples, function } => {
                // Use i32 keys (matching sample_triangles) and round rather
                // than truncate so the 0.5-offset centre point hits the
                // correct bucket and we avoid systematic off-by-one gaps
                // along triangle edges.
                let sample_point = (pos.x.round() as i32, pos.y.round() as i32);

                if let Some(color) = samples.get(&sample_point) {
                    if let Some(function) = function {
                        let val = function.eval(&color.to_smallvec())?;
                        Some(color_space.to_rgba(&val, 1.0, false))
                    } else {
                        Some(color_space.to_rgba(color, 1.0, false))
                    }
                } else {
                    Some(bg_color)
                }
            }
            Self::Dummy => Some(AlphaColor::TRANSPARENT),
        }
    }
}

fn ts_from_line_to_line(src1: Point, src2: Point, dst1: Point, dst2: Point) -> Affine {
    let unit_to_line1 = unit_to_line(src1, src2);
    let line1_to_unit = unit_to_line1.inverse();
    let unit_to_line2 = unit_to_line(dst1, dst2);

    unit_to_line2 * line1_to_unit
}

fn unit_to_line(p0: Point, p1: Point) -> Affine {
    Affine::new([
        p1.y - p0.y,
        p0.x - p1.x,
        p1.x - p0.x,
        p1.y - p0.y,
        p0.x,
        p0.y,
    ])
}

fn radial_pos(
    pos: &Point,
    p1: &Point,
    r: Point,
    min_extend: bool,
    max_extend: bool,
) -> Option<f32> {
    let r0 = r.x as f32;
    let dx = p1.x as f32;
    let dy = p1.y as f32;
    let dr = r.y as f32 - r0;

    let px = pos.x as f32;
    let py = pos.y as f32;

    let a = dx * dx + dy * dy - dr * dr;
    let b = -2.0 * (px * dx + py * dy + r0 * dr);
    let c = px * px + py * py - r0 * r0;

    let discriminant = b * b - 4.0 * a * c;

    // No solution available.
    if discriminant < 0.0 {
        return None;
    }

    if a.abs() < 1e-6 {
        if b.abs() < 1e-6 {
            return None;
        }

        let t = -c / b;

        if (!min_extend && t < 0.0) || (!max_extend && t > 1.0) {
            return None;
        }

        let r_t = r0 + dr * t;
        if r_t < 0.0 {
            return None;
        }

        return Some(t);
    }

    let sqrt_d = discriminant.sqrt();
    let t1 = (-b - sqrt_d) / (2.0 * a);
    let t2 = (-b + sqrt_d) / (2.0 * a);

    let max = t1.max(t2);
    let mut take_max = Some(max);
    let min = t1.min(t2);
    let mut take_min = Some(min);

    if (!min_extend && min < 0.0) || r0 + dr * min < 0.0 {
        take_min = None;
    }

    if (!max_extend && max > 1.0) || r0 + dr * max < 0.0 {
        take_max = None;
    }

    match (take_min, take_max) {
        (Some(_), Some(max)) => Some(max),
        (Some(min), None) => Some(min),
        (None, Some(max)) => Some(max),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shading::{Triangle, TriangleVertex};
    use kurbo::{Affine, Point};
    use rustc_hash::FxHashMap;
    use smallvec::smallvec;

    fn make_vertex(x: f64, y: f64, color: f32) -> TriangleVertex {
        TriangleVertex::new(0, Point::new(x, y), smallvec![color])
    }

    /// sample_triangles must use i32 keys so that negative coordinates (e.g.
    /// patterns starting above/left of the page origin) are stored and looked
    /// up consistently without wrapping at 0.
    #[test]
    fn sample_triangles_negative_coords() {
        let v0 = make_vertex(-2.0, -2.0, 0.0);
        let v1 = make_vertex(2.0, -2.0, 1.0);
        let v2 = make_vertex(0.0, 2.0, 0.5);
        let tri = Triangle::new(v0, v1, v2);

        let map = sample_triangles(&[tri], Affine::IDENTITY);

        // The map should contain entries for negative coordinate pixels.
        assert!(
            map.keys().any(|(x, _)| *x < 0),
            "expected negative x keys in sample map"
        );
        assert!(
            map.keys().any(|(_, y)| *y < 0),
            "expected negative y keys in sample map"
        );
    }

    /// The Sampled::eval lookup must match the i32 keys written by
    /// sample_triangles, and rounding must agree so samples are not missed.
    #[test]
    fn sampled_eval_roundtrip() {
        use crate::color::ColorSpace;

        let mut samples: FxHashMap<(i32, i32), ColorComponents> = FxHashMap::default();
        samples.insert((10, 20), smallvec![0.5]);

        let stype = EncodedShadingType::Sampled {
            samples,
            function: None,
        };

        let cs = ColorSpace::device_gray();
        let bg = AlphaColor::TRANSPARENT;

        // Exact integer lookup should find the sample.
        let hit = stype.eval(Point::new(10.0, 20.0), bg, &cs);
        assert!(hit.is_some(), "exact integer lookup should find sample");
        let color = hit.unwrap();
        assert!(color.components()[3] > 0.0, "sample should be opaque");

        // A point 0.4 away rounds to the same bucket.
        let hit2 = stype.eval(Point::new(10.4, 20.4), bg, &cs);
        assert!(
            hit2.is_some(),
            "nearby point (0.4 offset) should hit same bucket"
        );

        // A point 0.6 away rounds to (11, 21) which is not in the map → bg.
        let miss = stype.eval(Point::new(10.6, 20.6), bg, &cs);
        assert_eq!(
            miss.map(|c| c.components()[3]),
            Some(bg.components()[3]),
            "point rounding to (11,21) should return bg"
        );
    }
}

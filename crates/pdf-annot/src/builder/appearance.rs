//! Default appearance stream generation for each annotation subtype.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

#[cfg(feature = "write")]
use lopdf::Object;

#[cfg(feature = "write")]
use crate::appearance_writer::{AppearanceColor, AppearanceStreamBuilder};

#[cfg(feature = "write")]
use super::types::AnnotSubtype;

#[cfg(feature = "write")]
use super::AnnotationBuilder;

#[cfg(feature = "write")]
impl AnnotationBuilder {
    /// Generate a default appearance based on the annotation subtype.
    pub(super) fn default_appearance(&self, builder: &mut AppearanceStreamBuilder, w: f64, h: f64) {
        let stroke = self.color.unwrap_or(AppearanceColor::new(0.0, 0.0, 0.0));
        let needs_gs = self.opacity.is_some() || matches!(self.subtype, AnnotSubtype::Highlight);

        if needs_gs {
            builder.save_state();
            builder.ops_push_raw(lopdf::content::Operation::new(
                "gs",
                vec![Object::Name(b"GS0".to_vec())],
            ));
        }

        match self.subtype {
            AnnotSubtype::Square => {
                if let Some(ref fill) = self.interior_color {
                    builder.filled_stroked_rect(fill, &stroke, self.border_width);
                } else {
                    builder.stroked_rect(&stroke, self.border_width);
                }
            }
            AnnotSubtype::Circle => {
                builder.save_state();
                if let Some(ref fill) = self.interior_color {
                    builder.set_fill_color(fill);
                }
                builder.set_stroke_color(&stroke);
                builder.set_line_width(self.border_width);
                builder.ellipse();
                if self.interior_color.is_some() {
                    builder.fill_and_stroke();
                } else {
                    builder.stroke();
                }
                builder.restore_state();
            }
            AnnotSubtype::Line => {
                builder.save_state();
                builder.set_stroke_color(&stroke);
                builder.set_line_width(self.border_width);
                if let Some(ref dash) = self.dash_pattern {
                    builder.set_dash_pattern(dash, 0.0);
                }
                // Convert page coordinates to local form XObject coordinates.
                if let Some(ref l) = self.line_endpoints {
                    let lx1 = l[0] - self.rect.x0;
                    let ly1 = l[1] - self.rect.y0;
                    let lx2 = l[2] - self.rect.x0;
                    let ly2 = l[3] - self.rect.y0;
                    builder.line(lx1, ly1, lx2, ly2);
                } else {
                    builder.line(0.0, h / 2.0, w, h / 2.0);
                }
                builder.stroke();
                builder.restore_state();
            }
            AnnotSubtype::Highlight => {
                let fill = self.color.unwrap_or(AppearanceColor::new(1.0, 1.0, 0.0));
                builder.filled_rect(&fill);
            }
            AnnotSubtype::Underline => {
                builder.save_state();
                builder.set_stroke_color(&stroke);
                builder.set_line_width(self.border_width.max(0.5));
                builder.line(0.0, 0.0, w, 0.0);
                builder.stroke();
                builder.restore_state();
            }
            AnnotSubtype::StrikeOut => {
                builder.save_state();
                builder.set_stroke_color(&stroke);
                builder.set_line_width(self.border_width.max(0.5));
                builder.line(0.0, h / 2.0, w, h / 2.0);
                builder.stroke();
                builder.restore_state();
            }
            AnnotSubtype::Squiggly => {
                // Simplified squiggly: zigzag line at bottom.
                builder.save_state();
                builder.set_stroke_color(&stroke);
                builder.set_line_width(self.border_width.max(0.5));
                let step = 4.0;
                let amp = 2.0;
                builder.move_to(0.0, amp);
                let mut x = 0.0;
                let mut up = false;
                while x < w {
                    x += step;
                    let y = if up { amp } else { 0.0 };
                    builder.line_to(x.min(w), y);
                    up = !up;
                }
                builder.stroke();
                builder.restore_state();
            }
            AnnotSubtype::Ink => {
                builder.save_state();
                builder.set_stroke_color(&stroke);
                builder.set_line_width(self.border_width);
                if let Some(ref ink) = self.ink_list {
                    for path in ink {
                        if path.len() >= 2 {
                            let x0 = path[0] - self.rect.x0;
                            let y0 = path[1] - self.rect.y0;
                            builder.move_to(x0, y0);
                            let mut i = 2;
                            while i + 1 < path.len() {
                                let x = path[i] - self.rect.x0;
                                let y = path[i + 1] - self.rect.y0;
                                builder.line_to(x, y);
                                i += 2;
                            }
                            builder.stroke();
                        }
                    }
                }
                builder.restore_state();
            }
            AnnotSubtype::Polygon | AnnotSubtype::PolyLine => {
                builder.save_state();
                if let Some(ref fill) = self.interior_color {
                    builder.set_fill_color(fill);
                }
                builder.set_stroke_color(&stroke);
                builder.set_line_width(self.border_width);
                if let Some(ref dash) = self.dash_pattern {
                    builder.set_dash_pattern(dash, 0.0);
                }
                if let Some(ref verts) = self.vertices {
                    if verts.len() >= 2 {
                        let x0 = verts[0] - self.rect.x0;
                        let y0 = verts[1] - self.rect.y0;
                        builder.move_to(x0, y0);
                        let mut i = 2;
                        while i + 1 < verts.len() {
                            let x = verts[i] - self.rect.x0;
                            let y = verts[i + 1] - self.rect.y0;
                            builder.line_to(x, y);
                            i += 2;
                        }
                    }
                }
                let is_polygon = matches!(self.subtype, AnnotSubtype::Polygon);
                if is_polygon {
                    builder.close_path();
                    if self.interior_color.is_some() {
                        builder.fill_and_stroke();
                    } else {
                        builder.stroke();
                    }
                } else {
                    builder.stroke();
                }
                builder.restore_state();
            }
            AnnotSubtype::FreeText => {
                // White background with border.
                let white = AppearanceColor::new(1.0, 1.0, 1.0);
                builder.filled_stroked_rect(&white, &stroke, self.border_width);
                // Text is rendered via /DA by the viewer; the appearance stream
                // provides the background rectangle.
                if let Some(ref text) = self.contents {
                    let text_color = self.color.unwrap_or(AppearanceColor::new(0.0, 0.0, 0.0));
                    let margin = self.border_width + 2.0;
                    builder.text(text, "Helv", 12.0, margin, h - margin - 12.0, &text_color);
                }
            }
            AnnotSubtype::Text => {
                // Sticky note icon — simplified as a filled square with a border.
                let fill = AppearanceColor::new(1.0, 1.0, 0.6); // Light yellow
                builder.filled_stroked_rect(&fill, &stroke, self.border_width);
            }
            AnnotSubtype::Stamp => {
                // Stamp: red border with text.
                let red = AppearanceColor::new(1.0, 0.0, 0.0);
                builder.stroked_rect(&red, 2.0);
                if let Some(ref name) = self.icon_name {
                    builder.text(name, "Helv", 18.0, 4.0, h / 2.0 - 9.0, &red);
                }
            }
            AnnotSubtype::Link => {
                // Links are typically invisible — no appearance needed.
                // Empty appearance stream (viewer draws the link area).
            }
        }

        if needs_gs {
            builder.restore_state();
        }
    }
}

//! Appearance stream generation for PDF annotations.
//!
//! Generates Form XObject streams containing the visual representation
//! of annotations using PDF content stream operators.

#[cfg(feature = "write")]
use lopdf::content::Operation;
#[cfg(feature = "write")]
use lopdf::Object;

/// RGB color for annotation appearance.
#[cfg(feature = "write")]
#[derive(Debug, Clone, Copy)]
pub struct AppearanceColor {
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

#[cfg(feature = "write")]
impl AppearanceColor {
    pub fn new(r: f64, g: f64, b: f64) -> Self {
        Self { r, g, b }
    }

    /// Push fill color operators (rg).
    pub fn fill_ops(&self) -> Operation {
        Operation::new(
            "rg",
            vec![
                Object::Real(self.r as f32),
                Object::Real(self.g as f32),
                Object::Real(self.b as f32),
            ],
        )
    }

    /// Push stroke color operators (RG).
    pub fn stroke_ops(&self) -> Operation {
        Operation::new(
            "RG",
            vec![
                Object::Real(self.r as f32),
                Object::Real(self.g as f32),
                Object::Real(self.b as f32),
            ],
        )
    }
}

/// Builds content stream operations for annotation appearance streams.
///
/// All coordinates are in the Form XObject's local coordinate system,
/// where (0,0) is the bottom-left of the annotation rectangle.
#[cfg(feature = "write")]
pub struct AppearanceStreamBuilder {
    ops: Vec<Operation>,
    width: f64,
    height: f64,
}

#[cfg(feature = "write")]
impl AppearanceStreamBuilder {
    /// Create a new builder for an appearance stream with the given dimensions.
    pub fn new(width: f64, height: f64) -> Self {
        Self {
            ops: Vec::new(),
            width,
            height,
        }
    }

    /// Save the current graphics state.
    pub fn save_state(&mut self) -> &mut Self {
        self.ops.push(Operation::new("q", vec![]));
        self
    }

    /// Restore the graphics state.
    pub fn restore_state(&mut self) -> &mut Self {
        self.ops.push(Operation::new("Q", vec![]));
        self
    }

    /// Set the fill color (RGB).
    pub fn set_fill_color(&mut self, color: &AppearanceColor) -> &mut Self {
        self.ops.push(color.fill_ops());
        self
    }

    /// Set the stroke color (RGB).
    pub fn set_stroke_color(&mut self, color: &AppearanceColor) -> &mut Self {
        self.ops.push(color.stroke_ops());
        self
    }

    /// Set the line width for stroked paths.
    pub fn set_line_width(&mut self, width: f64) -> &mut Self {
        self.ops
            .push(Operation::new("w", vec![Object::Real(width as f32)]));
        self
    }

    /// Set the dash pattern for stroked paths.
    pub fn set_dash_pattern(&mut self, dash: &[f64], phase: f64) -> &mut Self {
        let arr: Vec<Object> = dash.iter().map(|&d| Object::Real(d as f32)).collect();
        self.ops.push(Operation::new(
            "d",
            vec![Object::Array(arr), Object::Real(phase as f32)],
        ));
        self
    }

    /// Draw a rectangle path.
    pub fn rect(&mut self, x: f64, y: f64, w: f64, h: f64) -> &mut Self {
        self.ops.push(Operation::new(
            "re",
            vec![
                Object::Real(x as f32),
                Object::Real(y as f32),
                Object::Real(w as f32),
                Object::Real(h as f32),
            ],
        ));
        self
    }

    /// Move to a point (start a new subpath).
    pub fn move_to(&mut self, x: f64, y: f64) -> &mut Self {
        self.ops.push(Operation::new(
            "m",
            vec![Object::Real(x as f32), Object::Real(y as f32)],
        ));
        self
    }

    /// Line to a point.
    pub fn line_to(&mut self, x: f64, y: f64) -> &mut Self {
        self.ops.push(Operation::new(
            "l",
            vec![Object::Real(x as f32), Object::Real(y as f32)],
        ));
        self
    }

    /// Cubic bezier curve.
    pub fn curve_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64) -> &mut Self {
        self.ops.push(Operation::new(
            "c",
            vec![
                Object::Real(x1 as f32),
                Object::Real(y1 as f32),
                Object::Real(x2 as f32),
                Object::Real(y2 as f32),
                Object::Real(x3 as f32),
                Object::Real(y3 as f32),
            ],
        ));
        self
    }

    /// Close the current subpath.
    pub fn close_path(&mut self) -> &mut Self {
        self.ops.push(Operation::new("h", vec![]));
        self
    }

    /// Stroke the current path.
    pub fn stroke(&mut self) -> &mut Self {
        self.ops.push(Operation::new("S", vec![]));
        self
    }

    /// Fill the current path (non-zero winding rule).
    pub fn fill(&mut self) -> &mut Self {
        self.ops.push(Operation::new("f", vec![]));
        self
    }

    /// Fill then stroke the current path.
    pub fn fill_and_stroke(&mut self) -> &mut Self {
        self.ops.push(Operation::new("B", vec![]));
        self
    }

    /// Close, fill and stroke.
    pub fn close_fill_and_stroke(&mut self) -> &mut Self {
        self.ops.push(Operation::new("b", vec![]));
        self
    }

    /// Add a filled rectangle covering the full annotation area.
    pub fn filled_rect(&mut self, color: &AppearanceColor) -> &mut Self {
        self.save_state();
        self.set_fill_color(color);
        self.rect(0.0, 0.0, self.width, self.height);
        self.fill();
        self.restore_state();
        self
    }

    /// Add a stroked rectangle (border) inside the annotation area.
    pub fn stroked_rect(&mut self, color: &AppearanceColor, line_width: f64) -> &mut Self {
        let half = line_width / 2.0;
        self.save_state();
        self.set_stroke_color(color);
        self.set_line_width(line_width);
        self.rect(
            half,
            half,
            self.width - line_width,
            self.height - line_width,
        );
        self.stroke();
        self.restore_state();
        self
    }

    /// Draw a filled and stroked rectangle.
    pub fn filled_stroked_rect(
        &mut self,
        fill: &AppearanceColor,
        stroke: &AppearanceColor,
        line_width: f64,
    ) -> &mut Self {
        let half = line_width / 2.0;
        self.save_state();
        self.set_fill_color(fill);
        self.set_stroke_color(stroke);
        self.set_line_width(line_width);
        self.rect(
            half,
            half,
            self.width - line_width,
            self.height - line_width,
        );
        self.fill_and_stroke();
        self.restore_state();
        self
    }

    /// Draw an ellipse (circle if width == height) using cubic bezier approximation.
    pub fn ellipse(&mut self) -> &mut Self {
        // Approximate ellipse with 4 cubic bezier curves.
        // Control point factor: 4*(sqrt(2)-1)/3 ≈ 0.5523
        let k = 0.5523;
        let cx = self.width / 2.0;
        let cy = self.height / 2.0;
        let rx = cx;
        let ry = cy;

        self.move_to(cx + rx, cy);
        self.curve_to(cx + rx, cy + ry * k, cx + rx * k, cy + ry, cx, cy + ry);
        self.curve_to(cx - rx * k, cy + ry, cx - rx, cy + ry * k, cx - rx, cy);
        self.curve_to(cx - rx, cy - ry * k, cx - rx * k, cy - ry, cx, cy - ry);
        self.curve_to(cx + rx * k, cy - ry, cx + rx, cy - ry * k, cx + rx, cy);
        self.close_path();
        self
    }

    /// Draw a line between two points in local coordinates.
    pub fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) -> &mut Self {
        self.move_to(x1, y1);
        self.line_to(x2, y2);
        self
    }

    /// Add text to the appearance stream.
    pub fn text(
        &mut self,
        text: &str,
        font_name: &str,
        font_size: f64,
        x: f64,
        y: f64,
        color: &AppearanceColor,
    ) -> &mut Self {
        self.save_state();
        self.set_fill_color(color);
        self.ops.push(Operation::new("BT", vec![]));
        self.ops.push(Operation::new(
            "Tf",
            vec![
                Object::Name(font_name.as_bytes().to_vec()),
                Object::Real(font_size as f32),
            ],
        ));
        self.ops.push(Operation::new(
            "Td",
            vec![Object::Real(x as f32), Object::Real(y as f32)],
        ));
        self.ops.push(Operation::new(
            "Tj",
            vec![Object::String(
                text.as_bytes().to_vec(),
                lopdf::StringFormat::Literal,
            )],
        ));
        self.ops.push(Operation::new("ET", vec![]));
        self.restore_state();
        self
    }

    /// Push a raw operation (for advanced use cases like ExtGState references).
    pub fn ops_push_raw(&mut self, op: Operation) -> &mut Self {
        self.ops.push(op);
        self
    }

    /// Encode the operations into content stream bytes.
    pub fn encode(self) -> Result<Vec<u8>, String> {
        lopdf::content::Content {
            operations: self.ops,
        }
        .encode()
        .map_err(|e| format!("{e}"))
    }

    /// Return the width of the appearance.
    pub fn width(&self) -> f64 {
        self.width
    }

    /// Return the height of the appearance.
    pub fn height(&self) -> f64 {
        self.height
    }
}

#[cfg(all(test, feature = "write"))]
mod tests {
    use super::*;

    #[test]
    fn encode_simple_rect() {
        let mut builder = AppearanceStreamBuilder::new(100.0, 50.0);
        let red = AppearanceColor::new(1.0, 0.0, 0.0);
        builder.stroked_rect(&red, 1.0);
        let bytes = builder.encode().unwrap();
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("RG"), "should contain stroke color");
        assert!(s.contains("re"), "should contain rectangle");
        assert!(s.contains("S"), "should contain stroke");
    }

    #[test]
    fn encode_filled_rect() {
        let mut builder = AppearanceStreamBuilder::new(80.0, 40.0);
        let yellow = AppearanceColor::new(1.0, 1.0, 0.0);
        builder.filled_rect(&yellow);
        let bytes = builder.encode().unwrap();
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("rg"), "should contain fill color");
        assert!(s.contains("f"), "should contain fill");
    }

    #[test]
    fn encode_ellipse() {
        let mut builder = AppearanceStreamBuilder::new(60.0, 60.0);
        let blue = AppearanceColor::new(0.0, 0.0, 1.0);
        builder.save_state();
        builder.set_stroke_color(&blue);
        builder.set_line_width(1.0);
        builder.ellipse();
        builder.stroke();
        builder.restore_state();
        let bytes = builder.encode().unwrap();
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("c"), "should contain bezier curves");
    }

    #[test]
    fn encode_text() {
        let mut builder = AppearanceStreamBuilder::new(200.0, 20.0);
        let black = AppearanceColor::new(0.0, 0.0, 0.0);
        builder.text("Hello World", "F1", 12.0, 2.0, 4.0, &black);
        let bytes = builder.encode().unwrap();
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("BT"), "should contain begin text");
        assert!(s.contains("Tj"), "should contain show text");
        assert!(s.contains("ET"), "should contain end text");
    }
}

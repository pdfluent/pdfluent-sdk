use kurbo::{Affine, BezPath};
use pdf_render::pdf_interpret::font::Glyph;
use pdf_render::pdf_interpret::{
    BlendMode, ClipPath, Device, FillRule, GlyphDrawMode, Image, Paint, PathDrawMode, SoftMask,
};
use xfa_wasm::canvas2d_device::{fill_rule_name, path_commands, CanvasPathCommand};

#[derive(Default)]
struct CommandRecorder {
    commands: Vec<String>,
}

impl CommandRecorder {
    fn record_path(&mut self, path: &BezPath) {
        self.commands.push("begin_path".to_string());
        for command in path_commands(path) {
            self.commands.push(format_path_command(command));
        }
    }
}

impl Device<'_> for CommandRecorder {
    fn set_soft_mask(&mut self, _: Option<SoftMask<'_>>) {}

    fn set_blend_mode(&mut self, _: BlendMode) {}

    fn draw_path(&mut self, path: &BezPath, _: Affine, _: &Paint<'_>, draw_mode: &PathDrawMode) {
        self.record_path(path);
        match draw_mode {
            PathDrawMode::Fill(fill_rule) => self
                .commands
                .push(format!("fill({})", fill_rule_name(*fill_rule))),
            PathDrawMode::Stroke(_) => self.commands.push("stroke".to_string()),
        }
    }

    fn push_clip_path(&mut self, clip_path: &ClipPath) {
        self.commands.push("save".to_string());
        self.record_path(&clip_path.path);
        self.commands
            .push(format!("clip({})", fill_rule_name(clip_path.fill)));
    }

    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'_>>, _: BlendMode) {}

    fn draw_glyph(
        &mut self,
        _: &Glyph<'_>,
        _: Affine,
        _: Affine,
        _: &Paint<'_>,
        _: &GlyphDrawMode,
    ) {
    }

    fn draw_image(&mut self, _: Image<'_, '_>, _: Affine) {}

    fn pop_clip_path(&mut self) {
        self.commands.push("restore".to_string());
    }

    fn pop_transparency_group(&mut self) {}
}

#[test]
fn moveto_lineto_closepath_produces_expected_sequence() {
    let mut path = BezPath::new();
    path.move_to((0.0, 0.0));
    path.line_to((10.0, 5.0));
    path.close_path();

    let mut recorder = CommandRecorder::default();
    recorder.draw_path(
        &path,
        Affine::IDENTITY,
        &Paint::Color(pdf_render::pdf_interpret::color::Color::from_device_rgb(
            0.0, 0.0, 0.0,
        )),
        &PathDrawMode::Fill(FillRule::NonZero),
    );

    assert_eq!(
        recorder.commands,
        vec![
            "begin_path",
            "move_to(0.0,0.0)",
            "line_to(10.0,5.0)",
            "close_path",
            "fill(nonzero)",
        ]
    );
}

#[test]
fn fill_rule_names_match_canvas_winding_rules() {
    let mut path = BezPath::new();
    path.move_to((1.0, 1.0));
    path.line_to((2.0, 2.0));

    let paint = Paint::Color(pdf_render::pdf_interpret::color::Color::from_device_rgb(
        1.0, 0.0, 0.0,
    ));
    let mut recorder = CommandRecorder::default();

    recorder.draw_path(
        &path,
        Affine::IDENTITY,
        &paint,
        &PathDrawMode::Fill(FillRule::NonZero),
    );
    recorder.draw_path(
        &path,
        Affine::IDENTITY,
        &paint,
        &PathDrawMode::Fill(FillRule::EvenOdd),
    );

    assert_eq!(
        recorder.commands,
        vec![
            "begin_path",
            "move_to(1.0,1.0)",
            "line_to(2.0,2.0)",
            "fill(nonzero)",
            "begin_path",
            "move_to(1.0,1.0)",
            "line_to(2.0,2.0)",
            "fill(evenodd)",
        ]
    );
}

#[test]
fn clip_path_is_wrapped_in_save_and_restore() {
    let mut path = BezPath::new();
    path.move_to((0.0, 0.0));
    path.line_to((4.0, 0.0));
    path.line_to((4.0, 4.0));
    path.close_path();

    let mut recorder = CommandRecorder::default();
    recorder.push_clip_path(&ClipPath {
        path,
        fill: FillRule::EvenOdd,
    });
    recorder.pop_clip_path();

    assert_eq!(
        recorder.commands,
        vec![
            "save",
            "begin_path",
            "move_to(0.0,0.0)",
            "line_to(4.0,0.0)",
            "line_to(4.0,4.0)",
            "close_path",
            "clip(evenodd)",
            "restore",
        ]
    );
}

fn format_path_command(command: CanvasPathCommand) -> String {
    match command {
        CanvasPathCommand::MoveTo(x, y) => format!("move_to({:.1},{:.1})", x, y),
        CanvasPathCommand::LineTo(x, y) => format!("line_to({:.1},{:.1})", x, y),
        CanvasPathCommand::QuadTo(cx, cy, x, y) => {
            format!("quad_to({:.1},{:.1},{:.1},{:.1})", cx, cy, x, y)
        }
        CanvasPathCommand::CurveTo(c1x, c1y, c2x, c2y, x, y) => format!(
            "curve_to({:.1},{:.1},{:.1},{:.1},{:.1},{:.1})",
            c1x, c1y, c2x, c2y, x, y
        ),
        CanvasPathCommand::ClosePath => "close_path".to_string(),
    }
}

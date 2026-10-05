use super::*;
use eframe::egui::{Event, Modifiers, PointerButton, Pos2, Rect, Shape};

fn frame(
    ctx: &egui::Context,
    lfos: &mut [LfoConfig; 2],
    which: usize,
    events: Vec<Event>,
) -> (egui::FullOutput, Rect) {
    let mut plot = Rect::NOTHING;
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(960.0, 720.0))),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.push_id(("lfo", which), |ui| {
                    // Match the actual LFO editor's sanitize-before-draw behavior.
                    lfos[which].sanitize();
                    plot = curve_editor(ui, &mut lfos[which]);
                });
            });
        },
    );
    (output, plot)
}
fn pointer(pos: Pos2, pressed: bool) -> Vec<Event> {
    vec![
        Event::PointerMoved(pos),
        Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        },
    ]
}
fn fixture() -> [LfoConfig; 2] {
    std::array::from_fn(|_| LfoConfig {
        shape: LfoShape::Custom,
        points: vec![
            CurvePoint {
                x: 0.0,
                y: 0.15,
                curve: 0.0,
            },
            CurvePoint {
                x: 0.5,
                y: 0.4,
                curve: 0.0,
            },
            CurvePoint {
                x: 0.75,
                y: 0.75,
                curve: 0.0,
            },
            CurvePoint {
                x: 1.0,
                y: 0.2,
                curve: 0.0,
            },
        ],
        ..Default::default()
    })
}
fn pos(plot: Rect, point: CurvePoint) -> Pos2 {
    pos2(
        plot.left() + plot.width() * point.x,
        plot.bottom() - plot.height() * point.y,
    )
}
fn verify_painted_nodes(output: &egui::FullOutput, plot: Rect, lfo: &LfoConfig) {
    let paths = output
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            Shape::Path(path) if path.stroke.width == 2.0 && path.points.len() > 20 => Some(path),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(paths.len(), 1, "Exactly one current LFO curve is painted");
    let nodes = output
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            Shape::Circle(c) if c.radius == 5.0 && plot.contains(c.center) => Some(c.center),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(nodes.len(), lfo.points.len());
    for (i, point) in lfo.points.iter().copied().enumerate() {
        let wanted = pos(plot, point);
        assert!(nodes.iter().any(|node| node.distance(wanted) < 0.01));
        assert!(
            paths[0].points.iter().any(|p| p.distance(wanted) < 0.01),
            "Curve must pass through even closely spaced node {i}"
        );
    }
}

#[test]
fn lfo_both_editors_keep_dragged_node_identity_at_left_right_boundaries() {
    for which in 0..2 {
        let ctx = egui::Context::default();
        let mut lfos = fixture();
        let other = lfos[1 - which].clone();
        frame(&ctx, &mut lfos, which, vec![]);
        let (_, plot) = frame(&ctx, &mut lfos, which, vec![]);
        let start = pos(plot, lfos[which].points[2]);
        frame(&ctx, &mut lfos, which, pointer(start, true));
        for (x, y) in [(0.40, 0.8), (0.35, 0.7), (1.1, 0.6), (0.7, 0.9)] {
            let to = pos2(
                plot.left() + plot.width() * x,
                plot.bottom() - plot.height() * y,
            );
            let (output, current_plot) =
                frame(&ctx, &mut lfos, which, vec![Event::PointerMoved(to)]);
            assert_eq!(
                lfos[which].points.len(),
                4,
                "Dragging cannot remove a point"
            );
            assert_eq!(
                (lfos[which].points[1].x, lfos[which].points[1].y),
                (0.5, 0.4)
            );
            assert!((lfos[which].points[2].y - y).abs() < 0.0001);
            assert!(lfos[which].points[2].x > 0.5 && lfos[which].points[2].x < 1.0);
            verify_painted_nodes(&output, current_plot, &lfos[which]);
        }
        let end = pos2(
            plot.left() + plot.width() * 0.7,
            plot.bottom() - plot.height() * 0.9,
        );
        frame(&ctx, &mut lfos, which, pointer(end, false));
        for _ in 0..3 {
            frame(&ctx, &mut lfos, which, vec![]);
        }
        assert_eq!(lfos[which].points.len(), 4);
        assert!((lfos[which].points[2].x - 0.7).abs() < 0.0001);
        assert_eq!(lfos[1 - which], other);
    }
}

#[test]
fn lfo_endpoints_remain_endpoints_when_dragged_across_each_other() {
    let ctx = egui::Context::default();
    let mut lfos = fixture();
    frame(&ctx, &mut lfos, 0, vec![]);
    let (_, plot) = frame(&ctx, &mut lfos, 0, vec![]);
    let start = pos(plot, lfos[0].points[3]);
    frame(&ctx, &mut lfos, 0, pointer(start, true));
    let to = pos2(plot.left() - 20.0, plot.center().y);
    let (output, plot) = frame(&ctx, &mut lfos, 0, vec![Event::PointerMoved(to)]);
    assert_eq!(lfos[0].points.len(), 4);
    assert_eq!(lfos[0].points[0].x, 0.0);
    assert_eq!(lfos[0].points[3].x, 1.0);
    assert_eq!(lfos[0].points[0].y, 0.15);
    verify_painted_nodes(&output, plot, &lfos[0]);
    frame(&ctx, &mut lfos, 0, pointer(to, false));
}

#[test]
fn sanitize_keeps_near_coincident_points_and_is_stable() {
    let mut lfo = fixture()[0].clone();
    lfo.points[2].x = lfo.points[1].x + LFO_POINT_GAP;
    assert!(
        lfo.points[2].x - lfo.points[1].x < LFO_POINT_GAP,
        "Exercise the old f32 dedup failure"
    );
    for _ in 0..100 {
        lfo.sanitize();
        assert_eq!(lfo.points.len(), 4);
        assert_eq!(lfo.points[2].y, 0.75);
        assert_eq!(lfo.points[1].y, 0.4);
    }
    let stable = lfo.clone();
    lfo.sanitize();
    assert_eq!(lfo, stable);
    lfo.points = (0..LFO_MAX_POINTS)
        .map(|i| CurvePoint {
            x: 1.0,
            y: i as f32 / LFO_MAX_POINTS as f32,
            curve: 0.0,
        })
        .collect();
    lfo.sanitize();
    assert_eq!(lfo.points.len(), LFO_MAX_POINTS);
    assert!(lfo.points.windows(2).all(|p| p[0].x < p[1].x));
    let stable = lfo.clone();
    lfo.sanitize();
    assert_eq!(lfo, stable);
}

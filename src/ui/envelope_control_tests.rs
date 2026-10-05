use super::*;
use crate::{app::Focus, ui::navigation};

#[test]
fn fractional_envelope_numeric_entry_and_one_ms_performance_keys() {
    let ctx = egui::Context::default();
    let mut config = EnvelopeConfigs::new();
    config.attack_ms.value = 1.3;
    let mut id = None;
    let mut frame = 0;
    let mut draw =
        |events: Vec<egui::Event>, value: &mut EnvelopeTime, id: &mut Option<egui::Id>| {
            let mut raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 200.0),
                )),
                time: Some(frame as f64 / 60.0),
                events,
                ..Default::default()
            };
            frame += 1;
            navigation::prepare_input(&ctx, &mut raw);
            let _ = ctx.run(raw, |ctx| {
                navigation::restore_input(ctx);
                egui::CentralPanel::default().show(ctx, |ui| {
                    navigation::begin(ui, Focus::Editor, false);
                    *id = Some(envelope_time(ui, value, 0.0, ENVELOPE_ATTACK_MAX_MS).id);
                    navigation::end(ui);
                });
            });
        };
    let key = |key, pressed| egui::Event::Key {
        key,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    draw(vec![], &mut config.attack_ms, &mut id);
    ctx.memory_mut(|m| m.request_focus(id.unwrap()));
    draw(vec![], &mut config.attack_ms, &mut id);
    draw(
        vec![key(egui::Key::ArrowRight, true)],
        &mut config.attack_ms,
        &mut id,
    );
    assert!((config.attack_ms.value - 2.3).abs() < 0.00001);
    draw(
        vec![key(egui::Key::ArrowRight, false)],
        &mut config.attack_ms,
        &mut id,
    );
    draw(
        vec![key(egui::Key::ArrowLeft, true)],
        &mut config.attack_ms,
        &mut id,
    );
    assert!((config.attack_ms.value - 1.3).abs() < 0.00001);
    draw(
        vec![key(egui::Key::ArrowLeft, false)],
        &mut config.attack_ms,
        &mut id,
    );
    draw(
        vec![key(egui::Key::Enter, true)],
        &mut config.attack_ms,
        &mut id,
    );
    draw(
        vec![key(egui::Key::Enter, false)],
        &mut config.attack_ms,
        &mut id,
    );
    draw(
        vec![egui::Event::Text("0.1".into())],
        &mut config.attack_ms,
        &mut id,
    );
    draw(
        vec![key(egui::Key::Enter, true)],
        &mut config.attack_ms,
        &mut id,
    );
    assert_eq!(config.attack_ms.value, 0.1);
    draw(
        vec![key(egui::Key::Enter, false)],
        &mut config.attack_ms,
        &mut id,
    );
    draw(
        vec![key(egui::Key::ArrowRight, true)],
        &mut config.attack_ms,
        &mut id,
    );
    assert!((config.attack_ms.value - 1.1).abs() < 0.00001);
}

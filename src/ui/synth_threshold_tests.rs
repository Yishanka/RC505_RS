use super::*;
use crate::app_support::language::Language;

fn texts(shape: &egui::Shape, output: &mut Vec<String>) {
    match shape {
        egui::Shape::Text(t) => output.push(t.galley.job.text.clone()),
        egui::Shape::Vec(shapes) => {
            for shape in shapes {
                texts(shape, output);
            }
        }
        _ => (),
    }
}
#[test]
fn osc_threshold_controls_are_distinct_and_capture_remains_visible_while_waiting() {
    for lang in [Language::English, Language::Chinese] {
        for full in [false, true] {
            for (wave, gate, armed) in [
                (Waveform::Sine, false, false),
                (Waveform::Sine, true, false),
                (Waveform::Sample, false, false),
                (Waveform::Sample, true, false),
                (Waveform::Sine, false, true),
                (Waveform::Sine, true, true),
            ] {
                let ctx = egui::Context::default();
                lang.apply(&ctx);
                let mut osc = OscillatorConfigs::new();
                osc.waveform.value = wave;
                osc.input_gate = gate;
                osc.gate_threshold.value = 13;
                osc.capture_threshold.value = 71;
                if armed {
                    osc.capture = Some(std::sync::Arc::new(SampleCapture::new(20)));
                }
                let mut painted = Vec::new();
                for _ in 0..2 {
                    let result = ctx.run(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                vec2(1100.0, 1400.0),
                            )),
                            ..Default::default()
                        },
                        |ctx| {
                            egui::CentralPanel::default().show(ctx, |ui| sound(ui, &mut osc, full));
                        },
                    );
                    painted.clear();
                    for shape in result.shapes {
                        texts(&shape.shape, &mut painted);
                    }
                }
                let gate_label = lang.text("Note gate threshold (%)");
                let capture_label = lang.text("Capture trigger threshold (%)");
                assert_eq!(
                    painted.iter().filter(|t| t.as_str() == gate_label).count(),
                    usize::from(gate),
                    "gate visibility: full={full}, armed={armed}"
                );
                assert_eq!(
                    painted
                        .iter()
                        .filter(|t| t.as_str() == capture_label)
                        .count(),
                    usize::from(wave == Waveform::Sample || armed),
                    "capture visibility: full={full}, armed={armed}"
                );
                assert_eq!(
                    (osc.gate_threshold.value, osc.capture_threshold.value),
                    (13, 71)
                );
            }
        }
    }
}

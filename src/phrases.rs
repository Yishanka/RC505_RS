//! Explicit phrase links share musical content, never oscillator sound settings.
//! Source identity stays attached to the slot when it is moved or disconnected.
use crate::{
    config::{
        AppConfig, InputFx,
        sequence_edit::{NoteClip, PendingClip},
    },
    presets::{self, FxTarget},
};

fn slot(config: &AppConfig, target: FxTarget) -> Option<&crate::config::input_fx_configs::FxSlot> {
    let FxTarget::Input { bank, slot } = target else {
        return None;
    };
    config.input_fx.banks.get(bank)?.slots.get(slot)
}
fn slot_mut(
    config: &mut AppConfig,
    target: FxTarget,
) -> Option<&mut crate::config::input_fx_configs::FxSlot> {
    let FxTarget::Input { bank, slot } = target else {
        return None;
    };
    config.input_fx.banks.get_mut(bank)?.slots.get_mut(slot)
}
pub fn stored_clip(config: &AppConfig, target: FxTarget) -> Option<NoteClip> {
    presets::clip(config, target).or_else(|| slot(config, target)?.clip.clone())
}
fn state(config: &AppConfig, target: FxTarget) -> Option<(NoteClip, u64, Option<PendingClip>)> {
    let slot = slot(config, target)?;
    match &slot.fx {
        Some(InputFx::Oscillator(osc)) => Some((
            osc.note.clip(),
            osc.note.launch_serial,
            osc.note.pending.clone(),
        )),
        _ => Some((stored_clip(config, target)?, 0, None)),
    }
}
pub fn members(config: &AppConfig, target: FxTarget) -> usize {
    let Some(link) = slot(config, target).and_then(|s| s.clip_link.as_ref()) else {
        return 1;
    };
    config
        .input_fx
        .banks
        .iter()
        .flat_map(|b| b.slots.iter())
        .filter(|s| s.clip_link.as_ref() == Some(link))
        .count()
}
pub fn propagate(config: &mut AppConfig, target: FxTarget) {
    let Some(link) = slot(config, target).and_then(|s| s.clip_link.clone()) else {
        return;
    };
    let Some((clip, serial, pending)) = state(config, target) else {
        return;
    };
    for bank in 0..4 {
        for index in 0..4 {
            let peer = FxTarget::Input { bank, slot: index };
            if peer == target
                || slot(config, peer).and_then(|s| s.clip_link.as_ref()) != Some(&link)
            {
                continue;
            }
            if let Some(note) = presets::note_mut(config, peer) {
                if let Some(next) = &pending {
                    if note.clip_id == next.clip.id {
                        // This peer reached its own boundary first. Shared edits
                        // must not queue the same phrase again and retrigger it.
                        note.set_clip(&next.clip);
                        note.pending = None;
                    } else {
                        if note.clip_id == clip.id {
                            note.set_clip(&clip);
                        }
                        note.pending = Some(next.clone());
                    }
                } else if let Some(waiting) = note.pending.as_mut().filter(|p| p.clip.id == clip.id)
                {
                    waiting.clip = clip.clone();
                } else {
                    let same = note.clip_id == clip.id;
                    note.set_clip(&clip);
                    if !same {
                        note.launch_serial = serial;
                        note.pending = None;
                    }
                }
            } else if let Some(slot) = slot_mut(config, peer) {
                slot.clip = Some(pending.as_ref().map_or(&clip, |p| &p.clip).clone());
            }
        }
    }
}
pub fn link(config: &mut AppConfig, source: FxTarget, target: FxTarget) -> anyhow::Result<()> {
    link_timed(config, source, target, false)
}
pub fn link_timed(
    config: &mut AppConfig,
    source: FxTarget,
    target: FxTarget,
    next_loop: bool,
) -> anyhow::Result<()> {
    anyhow::ensure!(source != target, "Choose another source");
    anyhow::ensure!(slot(config, target).is_some(), "Missing target");
    anyhow::ensure!(
        !next_loop || presets::note_mut(config, target).is_none_or(|n| n.pending.is_none()),
        "Cancel the queued phrase change first"
    );
    let (clip, serial, pending) =
        state(config, source).ok_or_else(|| anyhow::anyhow!("Source has no phrase"))?;
    let group = slot(config, source)
        .and_then(|s| s.clip_link.clone())
        .unwrap_or_else(|| format!("link-{}", crate::config::sequence_edit::phrase_serial()));
    slot_mut(config, source)
        .ok_or_else(|| anyhow::anyhow!("Missing source"))?
        .clip_link = Some(group.clone());
    let target_slot = slot_mut(config, target).ok_or_else(|| anyhow::anyhow!("Missing target"))?;
    target_slot.clip_link = Some(group);
    if let Some(note) = presets::note_mut(config, target) {
        if next_loop {
            note.launch_clip(pending.as_ref().map_or(&clip, |p| &p.clip), true);
        } else {
            note.set_clip(&clip);
            note.launch_serial = serial;
            note.pending = pending;
        }
    } else {
        slot_mut(config, target).unwrap().clip = Some(clip);
    }
    Ok(())
}
pub fn unlink(config: &mut AppConfig, target: FxTarget) {
    if let Some(slot) = slot_mut(config, target) {
        slot.clip_link = None;
    }
    if let Some(note) = presets::note_mut(config, target) {
        note.fork_clip_identity();
        if let Some(pending) = &mut note.pending {
            pending.clip.id = format!("clip-{}", crate::config::sequence_edit::phrase_serial());
        }
    }
}
pub fn copy_from(
    config: &mut AppConfig,
    source: FxTarget,
    target: FxTarget,
    next_loop: bool,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        !next_loop || presets::note_mut(config, target).is_none_or(|n| n.pending.is_none()),
        "Cancel the queued phrase change first"
    );
    let mut clip =
        stored_clip(config, source).ok_or_else(|| anyhow::anyhow!("Source has no phrase"))?;
    clip.id = format!("copy-{}", crate::config::sequence_edit::phrase_serial());
    unlink(config, target);
    if let Some(note) = presets::note_mut(config, target) {
        note.launch_clip(&clip, next_loop);
    } else {
        slot_mut(config, target)
            .ok_or_else(|| anyhow::anyhow!("Missing target"))?
            .clip = Some(clip);
    }
    Ok(())
}
pub fn cancel_pending(config: &mut AppConfig, target: FxTarget) {
    let link = slot(config, target).and_then(|s| s.clip_link.clone());
    for bank in 0..4 {
        for index in 0..4 {
            let peer = FxTarget::Input { bank, slot: index };
            if peer == target
                || link.as_ref().is_some_and(|link| {
                    slot(config, peer).and_then(|s| s.clip_link.as_ref()) == Some(link)
                })
            {
                if let Some(note) = presets::note_mut(config, peer) {
                    note.pending = None;
                }
            }
        }
    }
}
pub fn commit_applied(
    config: &mut AppConfig,
    views: &[[crate::dsp::oscillator::PhraseView; 4]; 4],
) {
    for (bank, group) in views.iter().enumerate() {
        for (index, view) in group.iter().enumerate() {
            let target = FxTarget::Input { bank, slot: index };
            if slot(config, target).is_some_and(|slot| {
                crate::dsp::oscillator::source_key(&slot.source_id) == view.source
            }) {
                let committed = presets::note_mut(config, target)
                    .is_some_and(|note| note.commit_pending(view.applied_serial));
                if committed {
                    propagate(config, target);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{FxKind, note_configs::NoteOct, sequence_edit::NoteEvent};
    #[test]
    fn explicit_link_roundtrips_and_unlink_keeps_an_independent_copy() {
        let mut config = AppConfig::new(120, 0, 5);
        let a = FxTarget::Input { bank: 0, slot: 0 };
        let b = FxTarget::Input { bank: 0, slot: 1 };
        for i in 0..2 {
            config.input_fx.set_slot_kind(0, i, FxKind::Oscillator);
        }
        presets::note_mut(&mut config, a).unwrap().replace_events(
            3840,
            &[NoteEvent::new(0, 960, NoteOct::from_pitch_index(48))],
        );
        link(&mut config, a, b).unwrap();
        assert_eq!(members(&config, a), 2);
        presets::note_mut(&mut config, b).unwrap().transpose(7);
        propagate(&mut config, b);
        assert_eq!(presets::clip(&config, a), presets::clip(&config, b));
        let data = crate::project::data_from_config(&config);
        let mut restored = AppConfig::new(120, 0, 5);
        crate::project::apply_data_to_config(
            &mut restored,
            serde_json::from_str(&serde_json::to_string(&data).unwrap()).unwrap(),
        );
        assert_eq!(members(&restored, b), 2);
        unlink(&mut restored, b);
        presets::note_mut(&mut restored, b).unwrap().transpose(12);
        propagate(&mut restored, b);
        assert_ne!(
            presets::clip(&restored, a).unwrap().events,
            presets::clip(&restored, b).unwrap().events
        );
        assert_ne!(
            presets::clip(&restored, a).unwrap().id,
            presets::clip(&restored, b).unwrap().id
        );
    }
    #[test]
    fn audio_switches_at_exact_old_loop_boundary_and_ui_ack_does_not_retrigger() {
        use crate::engine::{
            core::{Action, Parameters, RenderCore},
            loop_audio::OfflinePages,
        };
        let mut config = AppConfig::new(120, 0, 5);
        for bank in 0..2 {
            config.input_fx.set_slot_kind(bank, 0, FxKind::Oscillator);
            config.input_fx.banks[bank].slots[0].is_enabled = true;
            let note = presets::note_mut(&mut config, FxTarget::Input { bank, slot: 0 }).unwrap();
            note.replace_events(
                960,
                &[NoteEvent::new(0, 480, NoteOct::from_pitch_index(48))],
            );
        }
        let mut a = RenderCore::new(8000);
        let mut b = RenderCore::new(8000);
        a.configure(&mut Parameters::from_config(&config, 8000));
        b.configure(&mut Parameters::from_config(&config, 8000));
        a.action(Action::Metronome(true), &mut OfflinePages);
        b.action(Action::Metronome(true), &mut OfflinePages);
        let mut next = crate::config::note_configs::NoteConfigs::new();
        next.replace_events(
            1440,
            &[NoteEvent::new(0, 720, NoteOct::from_pitch_index(55))],
        );
        let mut serials = [0; 2];
        let mut acknowledged = false;
        for frame in 0..16000 {
            if frame == 1234 {
                for bank in 0..2 {
                    let note =
                        presets::note_mut(&mut config, FxTarget::Input { bank, slot: 0 }).unwrap();
                    note.launch_clip(&next.clip(), true);
                    serials[bank] = note.pending.as_ref().unwrap().serial;
                }
                a.configure(&mut Parameters::from_config(&config, 8000));
                b.configure(&mut Parameters::from_config(&config, 8000));
            }
            let mut out_a = [0.0; 2];
            let mut out_b = [0.0; 2];
            let allocations = crate::test_alloc::count(|| {
                out_a = a.process([0.0; 2], &mut OfflinePages);
                out_b = b.process([0.0; 2], &mut OfflinePages);
            });
            assert_eq!(
                allocations, 0,
                "Phrase boundary must not allocate at sample {frame}"
            );
            assert_eq!(
                out_a.map(f32::to_bits),
                out_b.map(f32::to_bits),
                "Ack changed audio at sample {frame}"
            );
            if frame == 3999 {
                assert_eq!(a.view().phrases[0][0].applied_serial, 0);
            }
            if frame == 4000 {
                let view = a.view();
                assert_eq!(view.phrases[0][0].applied_serial, serials[0]);
                assert_eq!(
                    view.phrases[1][0].applied_serial, serials[1],
                    "Inactive banks must advance pending phrase clocks too"
                );
                assert_eq!(view.phrases[0][0].origin_tick, 960);
                commit_applied(&mut config, &view.phrases);
                assert!(
                    presets::note_mut(&mut config, FxTarget::Input { bank: 0, slot: 0 })
                        .unwrap()
                        .pending
                        .is_none()
                );
                a.configure(&mut Parameters::from_config(&config, 8000));
                acknowledged = true;
            }
        }
        assert!(acknowledged);
    }
    #[test]
    fn stale_slot_ack_cannot_commit_moved_source_and_linked_edits_keep_launch_phase() {
        let mut config = AppConfig::new(120, 0, 5);
        let a = FxTarget::Input { bank: 0, slot: 0 };
        let b = FxTarget::Input { bank: 0, slot: 1 };
        for i in 0..2 {
            config.input_fx.set_slot_kind(0, i, FxKind::Oscillator);
        }
        let mut next = crate::config::note_configs::NoteConfigs::new();
        next.replace_events(
            960,
            &[NoteEvent::new(0, 480, NoteOct::from_pitch_index(60))],
        );
        presets::note_mut(&mut config, a)
            .unwrap()
            .replace_events(1920, &[]);
        presets::note_mut(&mut config, a)
            .unwrap()
            .launch_clip(&next.clip(), true);
        let serial = presets::note_mut(&mut config, a)
            .unwrap()
            .pending
            .as_ref()
            .unwrap()
            .serial;
        let mut views = [[crate::dsp::oscillator::PhraseView::default(); 4]; 4];
        views[0][0].source =
            crate::dsp::oscillator::source_key(&config.input_fx.banks[0].slots[0].source_id);
        views[0][0].applied_serial = serial;
        config.input_fx.banks[0].slots.swap(0, 1);
        commit_applied(&mut config, &views);
        assert_eq!(
            presets::note_mut(&mut config, b)
                .unwrap()
                .pending
                .as_ref()
                .unwrap()
                .serial,
            serial
        );
        views[0].swap(0, 1);
        commit_applied(&mut config, &views);
        assert!(presets::note_mut(&mut config, b).unwrap().pending.is_none());
        link(&mut config, b, a).unwrap();
        presets::note_mut(&mut config, a).unwrap().launch_serial = 12345;
        presets::note_mut(&mut config, b).unwrap().transpose(7);
        propagate(&mut config, b);
        assert_eq!(
            presets::note_mut(&mut config, a).unwrap().launch_serial,
            12345,
            "Ordinary shared edits must not restart peers"
        );
        assert_eq!(presets::clip(&config, a), presets::clip(&config, b));
    }
    #[test]
    fn linked_peers_crossing_different_boundaries_do_not_requeue_each_other() {
        let mut config = AppConfig::new(120, 0, 5);
        let a = FxTarget::Input { bank: 0, slot: 0 };
        let b = FxTarget::Input { bank: 0, slot: 1 };
        let c = FxTarget::Input { bank: 0, slot: 2 };
        for i in 0..3 {
            config.input_fx.set_slot_kind(0, i, FxKind::Oscillator);
        }
        presets::note_mut(&mut config, a).unwrap().replace_events(
            960,
            &[NoteEvent::new(0, 480, NoteOct::from_pitch_index(48))],
        );
        link(&mut config, a, b).unwrap();
        let mut next = crate::config::note_configs::NoteConfigs::new();
        next.replace_events(
            1440,
            &[NoteEvent::new(0, 480, NoteOct::from_pitch_index(55))],
        );
        presets::note_mut(&mut config, a)
            .unwrap()
            .launch_clip(&next.clip(), true);
        propagate(&mut config, a);
        let serial = presets::note_mut(&mut config, b)
            .unwrap()
            .pending
            .as_ref()
            .unwrap()
            .serial;
        assert!(
            presets::note_mut(&mut config, b)
                .unwrap()
                .commit_pending(serial)
        );
        presets::note_mut(&mut config, a).unwrap().transpose(1);
        propagate(&mut config, a);
        let peer = presets::note_mut(&mut config, b).unwrap();
        assert!(peer.pending.is_none());
        assert_eq!(peer.launch_serial, serial);
        assert_eq!(peer.clip().events, next.clip().events);
        presets::note_mut(&mut config, c)
            .unwrap()
            .replace_events(960, &[]);
        link_timed(&mut config, a, c, true).unwrap();
        assert_eq!(
            presets::note_mut(&mut config, c)
                .unwrap()
                .pending
                .as_ref()
                .unwrap()
                .clip
                .id,
            next.clip().id,
            "Joining a source with a queued change must follow its intended phrase"
        );
        presets::note_mut(&mut config, b).unwrap().transpose(7);
        propagate(&mut config, b);
        let expected = presets::clip(&config, b).unwrap().events;
        assert_eq!(
            presets::note_mut(&mut config, a)
                .unwrap()
                .pending
                .as_ref()
                .unwrap()
                .clip
                .events,
            expected
        );
        assert_eq!(
            presets::note_mut(&mut config, c)
                .unwrap()
                .pending
                .as_ref()
                .unwrap()
                .clip
                .events,
            expected
        );
    }
    #[test]
    fn copied_json_source_identities_are_repaired_deterministically() {
        let original = AppConfig::new(120, 0, 5);
        let mut data = crate::project::data_from_config(&original);
        for bank in &mut data.input_fx.banks {
            for slot in &mut bank.slots {
                slot.source_id = "manually-copied-source".into();
            }
        }
        let mut a = AppConfig::new(120, 0, 5);
        let mut b = AppConfig::new(120, 0, 5);
        crate::project::apply_data_to_config(&mut a, data.clone());
        crate::project::apply_data_to_config(&mut b, data);
        let ids = |c: &AppConfig| {
            c.input_fx
                .banks
                .iter()
                .flat_map(|b| b.slots.iter().map(|s| s.source_id.clone()))
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(&a), ids(&b));
        assert_eq!(
            ids(&a)
                .into_iter()
                .collect::<std::collections::HashSet<_>>()
                .len(),
            16
        );
        let roundtrip = crate::project::data_from_config(&a);
        crate::project::apply_data_to_config(&mut b, roundtrip);
        assert_eq!(ids(&a), ids(&b));
    }
}

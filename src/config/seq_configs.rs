use crate::config::config_type::EnumConfig;

const MAX_SEQ_LEN: usize = 12 * 32;
const TICKS_PER_BEAT: usize = 12;

pub struct SeqConfigs {
    seq: Vec<bool>,
    step_len_seq: Vec<usize>,
    pub sel_idx: Option<usize>,
    pub step: EnumConfig<String>,
    pub edit: EnumConfig<TrackSeqEdit>,
}

impl SeqConfigs {
    pub fn new() -> Self {
        Self {
            seq: vec![],
            step_len_seq: vec![],
            sel_idx: None,
            step: EnumConfig::new(
                "Step",
                "1/4".to_string(),
                vec![
                    "1/6".to_string(),
                    "1/4".to_string(),
                    "1/3".to_string(),
                    "1/2".to_string(),
                    "2/3".to_string(),
                    "3/4".to_string(),
                    "5/6".to_string(),
                    "1".to_string(),
                    "2".to_string(),
                ],
            ),
            edit: EnumConfig::new(
                "Seq",
                TrackSeqEdit::Push,
                vec![TrackSeqEdit::Push, TrackSeqEdit::Pop],
            ),
        }
    }

    pub fn seq(&self) -> &[bool] {
        &self.seq
    }

    pub fn step_len_seq(&self) -> &[usize] {
        &self.step_len_seq
    }

    pub fn set_seq(&mut self, seq: Vec<bool>) {
        self.step_len_seq = infer_step_len_seq(&seq);
        self.seq = seq;
    }

    pub fn set_seq_with_steps(&mut self, seq: Vec<bool>, step_len_seq: Vec<usize>) {
        if seq.len() != step_len_seq.len() {
            self.set_seq(seq);
            return;
        }
        self.seq = seq;
        self.step_len_seq = step_len_seq;
    }

    pub fn ticks_per_step(&self) -> usize {
        let per_beat = self.steps_per_beat().max(0.0001);
        ((TICKS_PER_BEAT as f32 / per_beat).round() as usize).max(1)
    }

    pub fn apply_edit(&mut self) {
        match self.edit.value {
            TrackSeqEdit::Push => self.push(),
            TrackSeqEdit::Pop => self.pop(),
        }
    }

    fn steps_per_beat(&self) -> f32 {
        let (num, den) = self.step_fraction();
        den as f32 / num as f32
    }

    fn step_fraction(&self) -> (usize, usize) {
        match self.step.value.as_str() {
            "1/6" => (1, 6),
            "1/4" => (1, 4),
            "1/3" => (1, 3),
            "1/2" => (1, 2),
            "2/3" => (2, 3),
            "3/4" => (3, 4),
            "5/6" => (5, 6),
            "1" => (1, 1),
            "2" => (2, 1),
            _ => (1, 4),
        }
    }

    fn push(&mut self) {
        let ticks = self.ticks_per_step().max(1);
        if self.seq.len() + ticks > MAX_SEQ_LEN {
            return;
        }
        for i in 0..ticks {
            self.seq.push(true);
            self.step_len_seq.push(if i == 0 { ticks } else { 0 });
        }
    }

    fn pop(&mut self) {
        if self.seq.is_empty() || self.step_len_seq.is_empty() {
            return;
        }

        if let Some((start, len)) = self
            .step_len_seq
            .iter()
            .enumerate()
            .rev()
            .find(|(_, v)| **v > 0)
            .map(|(idx, v)| (idx, *v))
        {
            let expected_end = start + len;
            if expected_end == self.seq.len() {
                self.seq.truncate(start);
                self.step_len_seq.truncate(start);
                return;
            }
        }

        self.seq.pop();
        self.step_len_seq = infer_step_len_seq(&self.seq);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TrackSeqEdit {
    Push,
    Pop,
}

impl std::fmt::Display for TrackSeqEdit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            TrackSeqEdit::Push => "Push",
            TrackSeqEdit::Pop => "Pop",
        };
        write!(f, "{}", label)
    }
}

impl crate::config::config_type::ConfigSet for SeqConfigs {
    fn next(&mut self) {
        let curr = self.sel_idx.unwrap_or(0);
        self.sel_idx = Some((curr + 1).min(1));
    }

    fn prev(&mut self) {
        let curr = self.sel_idx.unwrap_or(0);
        self.sel_idx = Some(curr.saturating_sub(1));
    }

    fn confirm(&mut self) {}
}

fn infer_step_len_seq(seq: &[bool]) -> Vec<usize> {
    let mut out = vec![0; seq.len()];
    let mut i = 0usize;
    while i < seq.len() {
        let mut j = i + 1;
        while j < seq.len() && seq[j] == seq[i] {
            j += 1;
        }
        out[i] = j - i;
        i = j;
    }
    out
}

//! One global input gate, with a physical threshold rather than opaque NS depth.
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InputNoiseConfig {
    pub enabled: bool,
    pub threshold_db: f32,
}
impl Default for InputNoiseConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            threshold_db: -50.0,
        }
    }
}
impl InputNoiseConfig {
    pub fn sanitized(self) -> Self {
        Self {
            enabled: self.enabled,
            threshold_db: if self.threshold_db.is_finite() {
                self.threshold_db.clamp(-80.0, 0.0)
            } else {
                -50.0
            },
        }
    }
}

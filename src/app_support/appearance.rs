use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThemeColor {
    Rose,
    Ember,
    #[default]
    #[serde(other)]
    Mint,
}

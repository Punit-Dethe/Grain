//! Trust levels for native runtime fixtures.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Trust {
    Dev,
    Experimental,
    Verified,
    Core,
}

impl Trust {
    pub const UNTRUSTED_DEFAULT: Trust = Trust::Dev;
}

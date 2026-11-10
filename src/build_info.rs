// SPDX-License-Identifier: GPL-3.0-or-later
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const ID: &str = env!("VYNULL_TOUCH_BUILD_ID");
pub const TARGET: &str = env!("VYNULL_TOUCH_TARGET");

pub fn description() -> String {
    format!("Vynull Touch {VERSION} · {ID} · {TARGET}")
}

// SPDX-License-Identifier: GPL-3.0

use cosmic::cosmic_config::{cosmic_config_derive::CosmicConfigEntry, CosmicConfigEntry};

#[derive(Debug, Clone, CosmicConfigEntry, Eq, PartialEq)]
#[version = 1]
pub struct Config {
    pub panel_label_max_length: u32,
    pub track_first: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            panel_label_max_length: 30,
            track_first: false,
        }
    }
}

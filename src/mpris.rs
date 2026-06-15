// SPDX-License-Identifier: GPL-3.0

use mpris::{PlaybackStatus, PlayerFinder};

#[derive(Debug, Clone)]
pub struct PlayerInfo {
    pub title: String,
    pub artist: String,
    pub status: PlaybackStatus,
    pub art_url: Option<String>,
    pub bus_name: String,
}

impl Default for PlayerInfo {
    fn default() -> Self {
        Self {
            title: String::new(),
            artist: String::new(),
            status: PlaybackStatus::Stopped,
            art_url: None,
            bus_name: String::new(),
        }
    }
}

/// Poll the active MPRIS player and return its current state.
/// Returns a default (empty) PlayerInfo when no player is found.
pub fn get_active_player_info() -> PlayerInfo {
    let Ok(finder) = PlayerFinder::new() else {
        return PlayerInfo::default();
    };
    let Ok(player) = finder.find_active() else {
        return PlayerInfo::default();
    };

    let metadata = player.get_metadata().unwrap_or_default();
    let status = player
        .get_playback_status()
        .unwrap_or(PlaybackStatus::Stopped);

    let title = metadata
        .title()
        .unwrap_or("Unknown")
        .to_string();

    let artist = metadata
        .artists()
        .map(|a| a.join(", "))
        .unwrap_or_default();

    let art_url = metadata.art_url().map(|u| u.to_string());
    let bus_name = player.bus_name_trimmed().to_string();

    PlayerInfo { title, artist, status, art_url, bus_name }
}

pub fn play_pause(bus_name: &str) {
    if let Ok(player) = find_by_bus(bus_name) {
        let _ = player.play_pause();
    }
}

pub fn next(bus_name: &str) {
    if let Ok(player) = find_by_bus(bus_name) {
        let _ = player.next();
    }
}

pub fn previous(bus_name: &str) {
    if let Ok(player) = find_by_bus(bus_name) {
        let _ = player.previous();
    }
}

fn find_by_bus(bus_name: &str) -> anyhow::Result<mpris::Player> {
    let finder = PlayerFinder::new()?;
    for player in finder.find_all()? {
        if player.bus_name_trimmed() == bus_name {
            return Ok(player);
        }
    }
    anyhow::bail!("player not found: {bus_name}")
}

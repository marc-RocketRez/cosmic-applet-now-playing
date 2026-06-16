// SPDX-License-Identifier: GPL-3.0

use mpris::{PlaybackStatus, Player, PlayerFinder};

/// `playerctld` is a proxy meta-player that mirrors whichever real player is
/// active. Because it shadows another player, including it in selection causes
/// flicker: a poll may resolve `playerctld` (whose forwarded metadata is briefly
/// empty between hand-offs) instead of the real player, blanking the panel for a
/// tick before flipping back. We always skip it and select among real players.
const PLAYERCTLD: &str = "playerctld";

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerInfo {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub year: Option<u32>,
    pub status: PlaybackStatus,
    pub art_url: Option<String>,
    pub bus_name: String,
    pub position_us: u64,
    pub length_us: u64,
}

impl Default for PlayerInfo {
    fn default() -> Self {
        Self {
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            year: None,
            status: PlaybackStatus::Stopped,
            art_url: None,
            bus_name: String::new(),
            position_us: 0,
            length_us: 0,
        }
    }
}

/// Poll the active MPRIS player and return its current state.
///
/// Returns `None` when no player could be resolved this tick — either there
/// genuinely is no player, or the lookup hit a transient D-Bus error (e.g. a
/// sibling player erroring mid-iteration inside `find_active`). Callers should
/// treat `None` as "no fresh data" rather than "stop playing", so a momentary
/// failure doesn't blank the panel.
pub fn get_active_player_info() -> Option<PlayerInfo> {
    let finder = PlayerFinder::new().ok()?;
    let player = find_active_real_player(&finder)?;

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

    let album = metadata.album_name().unwrap_or("").to_string();

    let year = metadata
        .get("xesam:year")
        .and_then(|v| v.as_i32().map(|n| n as u32).or_else(|| v.as_u32()))
        .or_else(|| {
            metadata
                .get("xesam:contentCreated")
                .and_then(|v| v.as_str())
                .and_then(|s| s.get(..4))
                .and_then(|s| s.parse::<u32>().ok())
        });

    let art_url = metadata.art_url().map(|u| u.to_string());
    let bus_name = player.bus_name_trimmed().to_string();
    let position_us = player.get_position_in_microseconds().unwrap_or(0);
    let length_us = metadata.length_in_microseconds().unwrap_or(0);

    Some(PlayerInfo { title, artist, album, year, status, art_url, bus_name, position_us, length_us })
}

/// Pick the active player among real players, excluding the `playerctld` proxy.
///
/// Mirrors the crate's `find_active` ordering (Playing > Paused > has-metadata >
/// first), but deterministically over the real players only, so we never flip to
/// the shadow proxy and blank the panel. Returns `None` on transient D-Bus errors
/// (treated by callers as "no fresh data", not "stopped").
fn find_active_real_player(finder: &PlayerFinder) -> Option<Player> {
    let players = finder.find_all().ok()?;

    let mut first_paused: Option<Player> = None;
    let mut first_with_track: Option<Player> = None;
    let mut first_found: Option<Player> = None;

    for player in players {
        if player.bus_name_trimmed() == PLAYERCTLD {
            continue;
        }
        let status = player.get_playback_status().unwrap_or(PlaybackStatus::Stopped);
        if status == PlaybackStatus::Playing {
            return Some(player);
        }
        if first_paused.is_none() && status == PlaybackStatus::Paused {
            first_paused = Some(player);
        } else if first_with_track.is_none()
            && player.get_metadata().map(|m| !m.is_empty()).unwrap_or(false)
        {
            first_with_track = Some(player);
        } else if first_found.is_none() {
            first_found = Some(player);
        }
    }

    first_paused.or(first_with_track).or(first_found)
}

pub fn seek_by(bus_name: &str, offset_us: i64) {
    if let Ok(player) = find_by_bus(bus_name) {
        let _ = player.seek(offset_us);
    }
}

pub fn seek_to(bus_name: &str, position_us: u64, current_position_us: u64) {
    if let Ok(player) = find_by_bus(bus_name) {
        let delta = position_us as i64 - current_position_us as i64;
        let _ = player.seek(delta);
    }
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

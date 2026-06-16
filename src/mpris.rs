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
    pub can_go_next: bool,
    pub can_go_previous: bool,
    pub can_pause: bool,
    pub can_play: bool,
    pub can_seek: bool,
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
            can_go_next: false,
            can_go_previous: false,
            can_pause: false,
            can_play: false,
            can_seek: false,
        }
    }
}

/// A lightweight entry for the player picker: enough to label and identify each
/// available MPRIS player without fetching its full state.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerSummary {
    pub bus_name: String,
    pub identity: String,
    pub status: PlaybackStatus,
    pub title: String,
}

/// One poll tick's worth of state: the player whose info to display (honoring a
/// pinned selection, else auto-picked) plus the list of all available players.
#[derive(Debug, Clone, Default)]
pub struct Poll {
    pub player: Option<PlayerInfo>,
    pub players: Vec<PlayerSummary>,
}

/// Poll MPRIS players once.
///
/// `selected` pins a specific player by trimmed bus name; if it's set and that
/// player is present, its info is returned. Otherwise we auto-pick the most
/// likely active player. `players` always lists every real player (excluding the
/// `playerctld` proxy) so the UI can offer a picker.
///
/// `player` is `None` when no player resolved this tick (genuinely none, or a
/// transient D-Bus error). Callers treat that as "no fresh data", not "stopped".
pub fn poll(selected: Option<&str>) -> Poll {
    let Ok(finder) = PlayerFinder::new() else { return Poll::default() };
    let Ok(all) = finder.find_all() else { return Poll::default() };

    // Drop the playerctld shadow proxy; it duplicates a real player.
    let real: Vec<Player> = all
        .into_iter()
        .filter(|p| p.bus_name_trimmed() != PLAYERCTLD)
        .collect();

    let players: Vec<PlayerSummary> = real
        .iter()
        .map(|p| PlayerSummary {
            bus_name: p.bus_name_trimmed().to_string(),
            identity: p.identity().to_string(),
            status: p.get_playback_status().unwrap_or(PlaybackStatus::Stopped),
            title: p
                .get_metadata()
                .ok()
                .and_then(|m| m.title().map(str::to_string))
                .unwrap_or_default(),
        })
        .collect();

    // Honor a pinned selection if it's still present, else auto-pick.
    let chosen = selected
        .and_then(|bus| players.iter().position(|s| s.bus_name == bus))
        .or_else(|| pick_active_index(&players));

    let player = chosen.map(|i| player_info(&real[i]));

    Poll { player, players }
}

/// Pick the index of the most likely active player from the summaries,
/// mirroring the MPRIS convention: Playing > Paused > has-a-track > first.
fn pick_active_index(players: &[PlayerSummary]) -> Option<usize> {
    let mut first_paused = None;
    let mut first_with_track = None;
    let mut first_found = None;

    for (i, s) in players.iter().enumerate() {
        if s.status == PlaybackStatus::Playing {
            return Some(i);
        }
        if first_paused.is_none() && s.status == PlaybackStatus::Paused {
            first_paused = Some(i);
        } else if first_with_track.is_none() && !s.title.is_empty() {
            first_with_track = Some(i);
        } else if first_found.is_none() {
            first_found = Some(i);
        }
    }

    first_paused.or(first_with_track).or(first_found)
}

/// Build the full `PlayerInfo` for one player (a batch of MPRIS property reads).
fn player_info(player: &Player) -> PlayerInfo {
    let metadata = player.get_metadata().unwrap_or_default();
    let status = player
        .get_playback_status()
        .unwrap_or(PlaybackStatus::Stopped);

    let title = metadata.title().unwrap_or("Unknown").to_string();

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
    let can_go_next = player.can_go_next().unwrap_or(false);
    let can_go_previous = player.can_go_previous().unwrap_or(false);
    let can_pause = player.can_pause().unwrap_or(false);
    let can_play = player.can_play().unwrap_or(false);
    let can_seek = player.can_seek().unwrap_or(false);

    PlayerInfo {
        title, artist, album, year, status, art_url, bus_name, position_us, length_us,
        can_go_next, can_go_previous, can_pause, can_play, can_seek,
    }
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

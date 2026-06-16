// SPDX-License-Identifier: GPL-3.0

use std::sync::LazyLock;
use std::time::Duration;

use cosmic::app::{Core, Task};
use cosmic::cosmic_config::{self, CosmicConfigEntry};
use cosmic::iced::widget::text::LineHeight;
use cosmic::iced::window::Id;
use cosmic::iced::{Alignment, Length, Limits, Subscription};
use cosmic::surface::action::{app_popup, destroy_popup};
use cosmic::widget;
use cosmic::Element;
use mpris::PlaybackStatus;

use crate::config::Config;
use crate::mpris::{PlayerInfo, PlayerSummary, Poll};

static PANEL_AUTOSIZE_ID: LazyLock<widget::Id> =
    LazyLock::new(|| widget::Id::new("now-playing-panel"));

/// Minimum panel height (in px) at which the two-line title/artist stack is used.
/// On thinner panels two lines become illegible, so we fall back to one line.
const MIN_STACK_PANEL_HEIGHT: u16 = 28;

/// Relative line height for the stacked panel text. >1.2 keeps descenders
/// (g/p/q/y tails) inside the line box rather than clipping them.
const STACK_LINE_HEIGHT: f32 = 1.3;

#[derive(Default)]
pub struct AppModel {
    core: Core,
    popup: Option<Id>,
    config: Config,
    player: PlayerInfo,
    album_art: Option<cosmic::iced::widget::image::Handle>,
    current_art_url: Option<String>,
    seeking: Option<f64>,
    missed_polls: u8,
    /// All MPRIS players seen on the last poll, for the picker.
    players: Vec<PlayerSummary>,
    /// User-pinned player (trimmed bus name); `None` means auto-pick.
    selected_player: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Message {
    TogglePopup,
    PopupClosed(Id),
    UpdateConfig(Config),
    Tick,
    Polled(Poll),
    SelectPlayer(String),
    ArtLoaded(Option<cosmic::iced::widget::image::Handle>),
    PlayPause,
    Next,
    Previous,
    LabelMaxLengthChanged(u32),
    TrackFirstChanged(bool),
    SeekChanged(f64),
    SeekCommit,
    SeekForward,
    SeekBackward,
}

impl cosmic::Application for AppModel {
    type Executor = cosmic::executor::Default;
    type Flags = ();
    type Message = Message;

    const APP_ID: &'static str = "io.github.cosmic-applet-now-playing";

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, _flags: ()) -> (Self, Task<Message>) {
        let config = cosmic_config::Config::new(Self::APP_ID, Config::VERSION)
            .map(|ctx| match Config::get_entry(&ctx) {
                Ok(cfg) => cfg,
                Err((_, cfg)) => cfg,
            })
            .unwrap_or_default();

        let app = AppModel { core, config, ..Default::default() };
        (app, Task::none())
    }

    fn on_close_requested(&self, id: Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn view(&self) -> Element<'_, Message> {
        let max_len = self.config.panel_label_max_length as usize;
        let panel_size = self.core.applet.suggested_size(false);
        let (_, vert_pad) = self.core.applet.suggested_padding(false);
        let thumb_size = panel_size.1;
        let height = (thumb_size + 2 * vert_pad) as f32;

        let button_content: Element<'_, Message> = if !self.player.bus_name.is_empty() {
            let mut children: Vec<Element<'_, Message>> = Vec::new();

            if let Some(ref handle) = self.album_art {
                children.push(
                    widget::image(handle.clone())
                        .width(Length::Fixed(thumb_size as f32))
                        .height(Length::Fixed(thumb_size as f32))
                        .content_fit(cosmic::iced::ContentFit::Cover)
                        .into(),
                );
            }

            let text_el: Element<'_, Message> = if !self.player.artist.is_empty()
                && thumb_size >= MIN_STACK_PANEL_HEIGHT
            {
                // Two-line stack: primary line on top, secondary below, with a
                // smaller font and tight line height so both fit the panel height.
                let (top, bottom) = if self.config.track_first {
                    (&self.player.title, &self.player.artist)
                } else {
                    (&self.player.artist, &self.player.title)
                };
                // Size the font from the actual available height so two line
                // boxes (font * line-height each) fit within the panel; the 0.9
                // factor leaves a little vertical breathing room around them.
                let line_size =
                    ((height * 0.9) / (2.0 * STACK_LINE_HEIGHT)).clamp(8.0, 13.0);
                widget::column(vec![
                    widget::text(truncate_label(top, max_len))
                        .size(line_size)
                        .line_height(LineHeight::Relative(STACK_LINE_HEIGHT))
                        .into(),
                    widget::text(truncate_label(bottom, max_len))
                        .size(line_size)
                        .line_height(LineHeight::Relative(STACK_LINE_HEIGHT))
                        .into(),
                ])
                .into()
            } else {
                let label_str = if self.player.artist.is_empty() {
                    self.player.title.clone()
                } else if self.config.track_first {
                    format!("{} \u{2014} {}", self.player.title, self.player.artist)
                } else {
                    format!("{} \u{2014} {}", self.player.artist, self.player.title)
                };
                self.core.applet.text(truncate_label(&label_str, max_len)).into()
            };
            children.push(text_el);

            widget::row(children)
                .spacing(6)
                .align_y(Alignment::Center)
                .into()
        } else {
            widget::icon::from_name("media-playback-stop-symbolic")
                .size(panel_size.0)
                .into()
        };

        use cosmic::iced::mouse;
        widget::autosize::autosize(
            widget::mouse_area(
                widget::button::custom(
                    widget::container(button_content)
                        .center_y(Length::Fixed(height)),
                )
                .height(Length::Fixed(height))
                .class(cosmic::theme::Button::AppletIcon)
                .on_press_down(Message::TogglePopup),
            )
            .on_scroll(|delta| {
                let y = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => y,
                    mouse::ScrollDelta::Pixels { y, .. } => y,
                };
                if y > 0.0 { Message::Next } else { Message::Previous }
            })
            .on_middle_press(Message::PlayPause),
            PANEL_AUTOSIZE_ID.clone(),
        )
        .into()
    }

    fn view_window(&self, _id: Id) -> Element<'_, Message> {
        // Popup is rendered via the app_popup closure in update(); this is a stub.
        widget::text("").into()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            cosmic::iced::time::every(Duration::from_millis(500)).map(|_| Message::Tick),
            self.core()
                .watch_config::<Config>(Self::APP_ID)
                .map(|update| Message::UpdateConfig(update.config)),
        ])
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::TogglePopup => {
                if let Some(id) = self.popup.take() {
                    return cosmic::task::message(cosmic::Action::Cosmic(
                        cosmic::app::Action::Surface(destroy_popup(id)),
                    ));
                } else {
                    return cosmic::task::message(cosmic::Action::Cosmic(
                        cosmic::app::Action::Surface(app_popup::<AppModel>(
                            |state: &mut AppModel| {
                                let new_id = Id::unique();
                                state.popup = Some(new_id);
                                state.core.applet.get_popup_settings(
                                    state.core.main_window_id().unwrap(),
                                    new_id,
                                    None,
                                    None,
                                    None,
                                )
                            },
                            Some(Box::new(|state: &AppModel| {
                                let spacing = cosmic::theme::active().cosmic().spacing;
                                let space_m: f32 = spacing.space_m.into();
                                let space_s: f32 = spacing.space_s.into();

                                let art: Element<'_, Message> =
                                    if let Some(ref handle) = state.album_art {
                                        widget::container(
                                            widget::image(handle.clone())
                                                .width(Length::Fixed(300.0))
                                                .height(Length::Fixed(300.0))
                                                .content_fit(cosmic::iced::ContentFit::Cover),
                                        )
                                        .width(Length::Fill)
                                        .align_x(cosmic::iced::alignment::Horizontal::Center)
                                        .into()
                                    } else {
                                        widget::container(
                                            widget::icon::from_name(
                                                "audio-headphones-symbolic",
                                            )
                                            .size(96),
                                        )
                                        .width(Length::Fixed(300.0))
                                        .height(Length::Fixed(300.0))
                                        .align_x(cosmic::iced::alignment::Horizontal::Center)
                                        .align_y(cosmic::iced::alignment::Vertical::Center)
                                        .class(cosmic::theme::Container::Card)
                                        .into()
                                    };

                                let status_icon = match &state.player.status {
                                    PlaybackStatus::Playing => "media-playback-pause-symbolic",
                                    _ => "media-playback-start-symbolic",
                                };

                                let seek_pos = state
                                    .seeking
                                    .unwrap_or(state.player.position_us as f64);

                                let progress: Element<'_, Message> =
                                    if state.player.length_us > 0 {
                                        let time_row = widget::row(vec![
                                            widget::text::caption(format_time(seek_pos as u64))
                                                .into(),
                                            widget::Space::new().width(Length::Fill).into(),
                                            widget::text::caption(format_time(
                                                state.player.length_us,
                                            ))
                                            .into(),
                                        ]);
                                        // Interactive slider when the player can
                                        // seek; otherwise a read-only progress bar.
                                        let bar: Element<'_, Message> = if state.player.can_seek {
                                            widget::slider(
                                                0.0..=state.player.length_us as f64,
                                                seek_pos,
                                                Message::SeekChanged,
                                            )
                                            .on_release(Message::SeekCommit)
                                            .width(Length::Fill)
                                            .into()
                                        } else {
                                            let frac = (seek_pos
                                                / state.player.length_us as f64)
                                                .clamp(0.0, 1.0)
                                                as f32;
                                            widget::container(widget::determinate_linear(frac))
                                                .width(Length::Fill)
                                                .into()
                                        };
                                        widget::column(vec![
                                            bar,
                                            time_row.into(),
                                        ])
                                        .spacing(2.0)
                                        .into()
                                    } else {
                                        widget::Space::new().width(Length::Fill).into()
                                    };

                                // Each transport button is only pressable when the
                                // player advertises the matching capability.
                                let mut control_row: Vec<Element<'_, Message>> = Vec::new();

                                let mut prev = widget::button::icon(widget::icon::from_name(
                                    "media-skip-backward-symbolic",
                                ));
                                if state.player.can_go_previous {
                                    prev = prev.on_press(Message::Previous);
                                }
                                control_row.push(prev.into());

                                let mut seek_back = widget::button::icon(widget::icon::from_name(
                                    "media-seek-backward-symbolic",
                                ));
                                if state.player.can_seek {
                                    seek_back = seek_back.on_press(Message::SeekBackward);
                                }
                                control_row.push(seek_back.into());

                                let can_playpause = match state.player.status {
                                    PlaybackStatus::Playing => state.player.can_pause,
                                    _ => state.player.can_play,
                                };
                                let mut play_pause =
                                    widget::button::icon(widget::icon::from_name(status_icon));
                                if can_playpause {
                                    play_pause = play_pause.on_press(Message::PlayPause);
                                }
                                control_row.push(play_pause.into());

                                let mut seek_fwd = widget::button::icon(widget::icon::from_name(
                                    "media-seek-forward-symbolic",
                                ));
                                if state.player.can_seek {
                                    seek_fwd = seek_fwd.on_press(Message::SeekForward);
                                }
                                control_row.push(seek_fwd.into());

                                let mut next = widget::button::icon(widget::icon::from_name(
                                    "media-skip-forward-symbolic",
                                ));
                                if state.player.can_go_next {
                                    next = next.on_press(Message::Next);
                                }
                                control_row.push(next.into());

                                let controls: Element<'_, Message> = widget::container(
                                    widget::row(control_row)
                                        .spacing(space_s)
                                        .align_y(Alignment::Center),
                                )
                                .width(Length::Fill)
                                .align_x(cosmic::iced::alignment::Horizontal::Center)
                                .into();

                                let label_spin = widget::settings::item(
                                    "Panel label length",
                                    cosmic::widget::spin_button(
                                        state.config.panel_label_max_length.to_string(),
                                        state.config.panel_label_max_length,
                                        1u32,
                                        10u32,
                                        100u32,
                                        Message::LabelMaxLengthChanged,
                                    ),
                                );

                                // Player picker: one selectable row per available
                                // player, shown only when there's a choice to make.
                                let player_picker: Option<Element<'_, Message>> =
                                    if state.players.len() > 1 {
                                        let rows: Vec<Element<'_, Message>> = state
                                            .players
                                            .iter()
                                            .map(|s| {
                                                let is_current =
                                                    s.bus_name == state.player.bus_name;
                                                let status_icon = match s.status {
                                                    PlaybackStatus::Playing => {
                                                        "media-playback-start-symbolic"
                                                    }
                                                    PlaybackStatus::Paused => {
                                                        "media-playback-pause-symbolic"
                                                    }
                                                    _ => "media-playback-stop-symbolic",
                                                };
                                                let label = if s.title.is_empty() {
                                                    s.identity.clone()
                                                } else {
                                                    format!("{} \u{2014} {}", s.identity, s.title)
                                                };
                                                widget::button::custom(
                                                    widget::row(vec![
                                                        widget::icon::from_name(status_icon)
                                                            .size(14)
                                                            .into(),
                                                        widget::text::body(truncate_label(
                                                            &label, 42,
                                                        ))
                                                        .into(),
                                                    ])
                                                    .spacing(space_s)
                                                    .align_y(Alignment::Center),
                                                )
                                                .selected(is_current)
                                                .width(Length::Fill)
                                                .on_press(Message::SelectPlayer(
                                                    s.bus_name.clone(),
                                                ))
                                                .into()
                                            })
                                            .collect();
                                        Some(widget::column(rows).spacing(2.0).into())
                                    } else {
                                        None
                                    };

                                let mut popup_children: Vec<Element<'_, Message>> = vec![
                                    label_spin.into(),
                                    widget::settings::item(
                                        "Track before artist",
                                        widget::toggler(state.config.track_first)
                                            .on_toggle(Message::TrackFirstChanged),
                                    )
                                    .into(),
                                    widget::divider::horizontal::default().into(),
                                ];
                                if let Some(picker) = player_picker {
                                    popup_children.push(picker);
                                }
                                popup_children.push(art);
                                popup_children
                                    .push(widget::text::title3(&state.player.title).into());
                                popup_children
                                    .push(widget::text::body(&state.player.artist).into());
                                if !state.player.album.is_empty() {
                                    let album_str = if let Some(y) = state.player.year {
                                        format!("{} ({})", state.player.album, y)
                                    } else {
                                        state.player.album.clone()
                                    };
                                    popup_children.push(widget::text::caption(album_str).into());
                                }
                                popup_children.push(progress);
                                popup_children.push(controls);

                                let content = widget::column(popup_children)
                                .spacing(space_s)
                                .padding(space_m);

                                Element::from(
                                    state
                                        .core
                                        .applet
                                        .popup_container(content)
                                        .limits(
                                            Limits::NONE
                                                .min_width(320.0)
                                                .max_width(400.0)
                                                .min_height(200.0)
                                                .max_height(750.0),
                                        ),
                                )
                                .map(cosmic::Action::App)
                            })),
                        )),
                    ));
                }
            }

            Message::PopupClosed(id) => {
                if self.popup.as_ref() == Some(&id) {
                    self.popup = None;
                }
            }

            Message::UpdateConfig(config) => {
                self.config = config;
            }

            Message::Tick => {
                let selected = self.selected_player.clone();
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            crate::mpris::poll(selected.as_deref())
                        })
                        .await
                        .unwrap_or_default()
                    },
                    |poll| cosmic::Action::App(Message::Polled(poll)),
                );
            }

            Message::SelectPlayer(bus_name) => {
                // Pin the chosen player. Reset per-player UI state so we don't show
                // the previous player's art/seek until the next poll lands.
                self.selected_player = Some(bus_name);
                self.album_art = None;
                self.current_art_url = None;
                self.seeking = None;
            }

            Message::Polled(poll) => {
                self.players = poll.players;
                // Drop a pinned selection whose player has disappeared, so we fall
                // back to auto-pick rather than getting stuck on a dead player.
                if let Some(sel) = &self.selected_player {
                    if !self.players.iter().any(|p| &p.bus_name == sel) {
                        self.selected_player = None;
                    }
                }

                let Some(info) = poll.player else {
                    // Transient lookup miss. Keep the last-known player for a few
                    // ticks; only clear once we're confident it's really gone.
                    if !self.player.bus_name.is_empty() {
                        self.missed_polls = self.missed_polls.saturating_add(1);
                        if self.missed_polls >= 4 {
                            self.player = PlayerInfo::default();
                            self.album_art = None;
                            self.current_art_url = None;
                            self.seeking = None;
                        }
                    }
                    return Task::none();
                };

                self.missed_polls = 0;

                // Clear the seek-pin once the player position is within 2s of the
                // target. This runs before the unchanged-state short-circuit below
                // so it still fires for players that report identical state ticks.
                if let Some(target) = self.seeking {
                    if (info.position_us as f64 - target).abs() < 2_000_000.0 {
                        self.seeking = None;
                    }
                }

                if info == self.player {
                    return Task::none();
                }
                let new_art_url = info.art_url.clone();
                self.player = info;

                if new_art_url != self.current_art_url {
                    self.current_art_url = new_art_url.clone();
                    self.album_art = None;
                    if let Some(url) = new_art_url {
                        return Task::perform(
                            load_art(url),
                            |handle| cosmic::Action::App(Message::ArtLoaded(handle)),
                        );
                    }
                }
            }

            Message::ArtLoaded(handle) => {
                self.album_art = handle;
            }

            Message::PlayPause => {
                crate::mpris::play_pause(&self.player.bus_name);
            }

            Message::Next => {
                crate::mpris::next(&self.player.bus_name);
            }

            Message::Previous => {
                crate::mpris::previous(&self.player.bus_name);
            }

            Message::SeekChanged(pos) => {
                self.seeking = Some(pos);
            }

            Message::SeekCommit => {
                if let Some(pos) = self.seeking {
                    // Keep `seeking` set — the slider stays pinned to the target
                    // until the next poll confirms the position has caught up.
                    let target = pos as u64;
                    let bus_name = self.player.bus_name.clone();
                    let current = self.player.position_us;
                    tokio::task::spawn_blocking(move || {
                        crate::mpris::seek_to(&bus_name, target, current)
                    });
                }
            }

            Message::SeekForward => {
                let bus_name = self.player.bus_name.clone();
                tokio::task::spawn_blocking(move || {
                    crate::mpris::seek_by(&bus_name, 10_000_000)
                });
            }

            Message::SeekBackward => {
                let bus_name = self.player.bus_name.clone();
                tokio::task::spawn_blocking(move || {
                    crate::mpris::seek_by(&bus_name, -10_000_000)
                });
            }

            Message::LabelMaxLengthChanged(len) => {
                self.config.panel_label_max_length = len;
                if let Ok(ctx) = cosmic_config::Config::new(Self::APP_ID, Config::VERSION) {
                    let _ = self.config.write_entry(&ctx);
                }
            }

            Message::TrackFirstChanged(val) => {
                self.config.track_first = val;
                if let Ok(ctx) = cosmic_config::Config::new(Self::APP_ID, Config::VERSION) {
                    let _ = self.config.write_entry(&ctx);
                }
            }

        }

        Task::none()
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }
}

fn format_time(us: u64) -> String {
    let secs = us / 1_000_000;
    let mins = secs / 60;
    format!("{}:{:02}", mins, secs % 60)
}

fn truncate_label(s: &str, max_chars: usize) -> String {
    let mut chars = s.chars();
    let truncated: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{truncated}\u{2026}")
    } else {
        truncated
    }
}

async fn load_art(url: String) -> Option<cosmic::iced::widget::image::Handle> {
    let bytes: Option<Vec<u8>> = if let Some(path) = url.strip_prefix("file://") {
        tokio::fs::read(path).await.ok()
    } else {
        match reqwest::get(&url).await {
            Ok(resp) => resp.bytes().await.ok().map(|b| b.to_vec()),
            Err(_) => None,
        }
    };
    bytes.map(cosmic::iced::widget::image::Handle::from_bytes)
}

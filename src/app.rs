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
use crate::mpris::PlayerInfo;

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
}

#[derive(Debug, Clone)]
pub enum Message {
    TogglePopup,
    PopupClosed(Id),
    UpdateConfig(Config),
    Tick,
    PlayerUpdated(Option<crate::mpris::PlayerInfo>),
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
                                        let slider = widget::slider(
                                            0.0..=state.player.length_us as f64,
                                            seek_pos,
                                            Message::SeekChanged,
                                        )
                                        .on_release(Message::SeekCommit)
                                        .width(Length::Fill);
                                        widget::column(vec![
                                            slider.into(),
                                            time_row.into(),
                                        ])
                                        .spacing(2.0)
                                        .into()
                                    } else {
                                        widget::Space::new().width(Length::Fill).into()
                                    };

                                let controls: Element<'_, Message> = widget::container(
                                    widget::row(vec![
                                        widget::button::icon(widget::icon::from_name(
                                            "media-skip-backward-symbolic",
                                        ))
                                        .on_press(Message::Previous)
                                        .into(),
                                        widget::button::icon(widget::icon::from_name(
                                            "media-seek-backward-symbolic",
                                        ))
                                        .on_press(Message::SeekBackward)
                                        .into(),
                                        widget::button::icon(widget::icon::from_name(status_icon))
                                            .on_press(Message::PlayPause)
                                            .into(),
                                        widget::button::icon(widget::icon::from_name(
                                            "media-seek-forward-symbolic",
                                        ))
                                        .on_press(Message::SeekForward)
                                        .into(),
                                        widget::button::icon(widget::icon::from_name(
                                            "media-skip-forward-symbolic",
                                        ))
                                        .on_press(Message::Next)
                                        .into(),
                                    ])
                                    .spacing(space_m)
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

                                let mut popup_children: Vec<Element<'_, Message>> = vec![
                                    label_spin.into(),
                                    widget::settings::item(
                                        "Track before artist",
                                        widget::toggler(state.config.track_first)
                                            .on_toggle(Message::TrackFirstChanged),
                                    )
                                    .into(),
                                    widget::divider::horizontal::default().into(),
                                    art,
                                    widget::text::title3(&state.player.title).into(),
                                    widget::text::body(&state.player.artist).into(),
                                ];
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
                return Task::perform(
                    async {
                        tokio::task::spawn_blocking(crate::mpris::get_active_player_info)
                            .await
                            .ok()
                            .flatten()
                    },
                    |info| cosmic::Action::App(Message::PlayerUpdated(info)),
                );
            }

            Message::PlayerUpdated(None) => {
                // Transient lookup miss. Keep showing the last-known player for a
                // few ticks; only clear once we're confident it's really gone.
                if !self.player.bus_name.is_empty() {
                    self.missed_polls = self.missed_polls.saturating_add(1);
                    if self.missed_polls >= 4 {
                        self.player = PlayerInfo::default();
                        self.album_art = None;
                        self.current_art_url = None;
                        self.seeking = None;
                    }
                }
            }

            Message::PlayerUpdated(Some(info)) => {
                self.missed_polls = 0;
                if info == self.player {
                    return Task::none();
                }
                // Clear seek-pin once the player position is within 2s of the target.
                if let Some(target) = self.seeking {
                    if (info.position_us as f64 - target).abs() < 2_000_000.0 {
                        self.seeking = None;
                    }
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
                    // until PlayerUpdated confirms the position has caught up.
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

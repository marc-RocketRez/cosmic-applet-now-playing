// SPDX-License-Identifier: GPL-3.0

use std::sync::LazyLock;
use std::time::Duration;

use cosmic::app::Task;
use cosmic::cosmic_config::{self, CosmicConfigEntry};
use cosmic::iced::{window::Id, Limits, Subscription};
use cosmic::iced_winit::commands::popup::{destroy_popup, get_popup};
use cosmic::prelude::*;
use cosmic::widget;
use mpris::PlaybackStatus;

use crate::config::Config;
use crate::mpris::PlayerInfo;

static PANEL_AUTOSIZE_ID: LazyLock<widget::Id> =
    LazyLock::new(|| widget::Id::new("now-playing-panel"));

#[derive(Default)]
pub struct AppModel {
    core: cosmic::Core,
    popup: Option<Id>,
    config: Config,
    player: PlayerInfo,
    album_art: Option<cosmic::iced::widget::image::Handle>,
    current_art_url: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Message {
    TogglePopup,
    PopupClosed(Id),
    UpdateConfig(Config),
    Tick,
    LoadArt(String),
    ArtLoaded(Option<cosmic::iced::widget::image::Handle>),
    PlayPause,
    Next,
    Previous,
    LabelMaxLengthChanged(u32),
}

impl cosmic::Application for AppModel {
    type Executor = cosmic::executor::Default;
    type Flags = ();
    type Message = Message;

    const APP_ID: &'static str = "io.github.cosmic-applet-now-playing";

    fn core(&self) -> &cosmic::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

    fn init(core: cosmic::Core, _flags: ()) -> (Self, Task<cosmic::Action<Message>>) {
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
            let label_str = if self.player.artist.is_empty() {
                self.player.title.clone()
            } else {
                format!("{} \u{2014} {}", self.player.artist, self.player.title)
            };
            let label = truncate_label(&label_str, max_len);

            let mut row = widget::row()
                .spacing(6)
                .align_y(cosmic::iced::Alignment::Center);

            if let Some(ref handle) = self.album_art {
                row = row.push(
                    widget::image(handle.clone())
                        .width(cosmic::iced::Length::Fixed(thumb_size as f32))
                        .height(cosmic::iced::Length::Fixed(thumb_size as f32))
                        .content_fit(cosmic::iced::ContentFit::Cover),
                );
            }

            row = row.push(self.core.applet.text(label));
            row.into()
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
                        .center_y(cosmic::iced::Length::Fixed(height)),
                )
                .height(cosmic::iced::Length::Fixed(height))
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
        let cosmic::cosmic_theme::Spacing { space_s, space_m, .. } =
            cosmic::theme::active().cosmic().spacing;

        // Album art — 300×300 minimum
        let art: Element<'_, Message> = if let Some(ref handle) = self.album_art {
            widget::container(
                widget::image(handle.clone())
                    .width(cosmic::iced::Length::Fixed(300.0))
                    .height(cosmic::iced::Length::Fixed(300.0))
                    .content_fit(cosmic::iced::ContentFit::Cover),
            )
            .width(cosmic::iced::Length::Fill)
            .align_x(cosmic::iced::alignment::Horizontal::Center)
            .into()
        } else {
            widget::container(
                widget::icon::from_name("audio-headphones-symbolic").size(96),
            )
            .width(cosmic::iced::Length::Fixed(300.0))
            .height(cosmic::iced::Length::Fixed(300.0))
            .align_x(cosmic::iced::alignment::Horizontal::Center)
            .align_y(cosmic::iced::alignment::Vertical::Center)
            .class(cosmic::theme::Container::Card)
            .into()
        };

        // Track + artist
        let track_info = widget::column()
            .spacing(space_s)
            .push(widget::text::title3(&self.player.title))
            .push(widget::text::body(&self.player.artist))
            .align_x(cosmic::iced::Alignment::Center)
            .width(cosmic::iced::Length::Fill);

        // Playback controls
        let status_icon = match self.player.status {
            PlaybackStatus::Playing => "media-playback-pause-symbolic",
            _ => "media-playback-start-symbolic",
        };

        let controls = widget::row()
            .spacing(space_m)
            .align_y(cosmic::iced::Alignment::Center)
            .push(
                widget::button::icon(widget::icon::from_name("media-skip-backward-symbolic"))
                    .on_press(Message::Previous),
            )
            .push(
                widget::button::icon(widget::icon::from_name(status_icon))
                    .on_press(Message::PlayPause),
            )
            .push(
                widget::button::icon(widget::icon::from_name("media-skip-forward-symbolic"))
                    .on_press(Message::Next),
            );

        // Panel label length setting
        let label_spin = cosmic::widget::spin_button(
            "Panel label length",
            self.config.panel_label_max_length,
            1u32,
            10u32,
            100u32,
            Message::LabelMaxLengthChanged,
        );

        let content = widget::column()
            .spacing(space_m)
            .padding(space_m)
            .push(art)
            .push(track_info)
            .push(
                widget::container(controls)
                    .width(cosmic::iced::Length::Fill)
                    .align_x(cosmic::iced::alignment::Horizontal::Center),
            )
            .push(widget::divider::horizontal::default())
            .push(label_spin);

        self.core
            .applet
            .popup_container(content)
            .limits(
                Limits::NONE
                    .min_width(320.0)
                    .max_width(400.0)
                    .min_height(200.0)
                    .max_height(750.0),
            )
            .into()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            cosmic::iced::time::every(Duration::from_millis(500)).map(|_| Message::Tick),
            self.core()
                .watch_config::<Config>(Self::APP_ID)
                .map(|update| Message::UpdateConfig(update.config)),
        ])
    }

    fn update(&mut self, message: Message) -> Task<cosmic::Action<Message>> {
        match message {
            Message::TogglePopup => {
                return if let Some(p) = self.popup.take() {
                    destroy_popup(p)
                } else {
                    let new_id = Id::unique();
                    self.popup.replace(new_id);
                    let mut settings = self.core.applet.get_popup_settings(
                        self.core.main_window_id().unwrap(),
                        new_id,
                        None,
                        None,
                        None,
                    );
                    settings.positioner.size_limits = Limits::NONE
                        .min_width(320.0)
                        .max_width(400.0)
                        .min_height(200.0)
                        .max_height(750.0);
                    get_popup(settings)
                };
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
                let info = crate::mpris::get_active_player_info();
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

            Message::LoadArt(url) => {
                return Task::perform(
                    load_art(url),
                    |handle| cosmic::Action::App(Message::ArtLoaded(handle)),
                );
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

            Message::LabelMaxLengthChanged(len) => {
                self.config.panel_label_max_length = len;
                if let Ok(ctx) = cosmic_config::Config::new(Self::APP_ID, Config::VERSION) {
                    let _ = self.config.write_entry(&ctx);
                }
            }
        }

        Task::none()
    }

    fn style(&self) -> Option<cosmic::iced_runtime::Appearance> {
        Some(cosmic::applet::style())
    }
}

fn truncate_label(s: &str, max_chars: usize) -> String {
    let mut chars = s.chars();
    let truncated: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{truncated}\u{2026}") // …
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

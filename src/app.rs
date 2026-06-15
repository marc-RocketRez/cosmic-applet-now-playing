// SPDX-License-Identifier: GPL-3.0

use std::sync::LazyLock;
use std::time::Duration;

use cosmic::app::{Core, Task};
use cosmic::cosmic_config::{self, CosmicConfigEntry};
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

#[derive(Default)]
pub struct AppModel {
    core: Core,
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
    Surface(cosmic::surface::Action),
    Tick,
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
            let label_str = if self.player.artist.is_empty() {
                self.player.title.clone()
            } else {
                format!("{} \u{2014} {}", self.player.artist, self.player.title)
            };
            let label = truncate_label(&label_str, max_len);

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

            children.push(self.core.applet.text(label).into());

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

                                let controls: Element<'_, Message> = widget::container(
                                    widget::row(vec![
                                        widget::button::icon(widget::icon::from_name(
                                            "media-skip-backward-symbolic",
                                        ))
                                        .on_press(Message::Previous)
                                        .into(),
                                        widget::button::icon(widget::icon::from_name(status_icon))
                                            .on_press(Message::PlayPause)
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

                                let label_spin = cosmic::widget::spin_button(
                                    "Panel label length",
                                    state.config.panel_label_max_length,
                                    1u32,
                                    10u32,
                                    100u32,
                                    Message::LabelMaxLengthChanged,
                                );

                                let content = widget::column(vec![
                                    art,
                                    widget::text::title3(&state.player.title).into(),
                                    widget::text::body(&state.player.artist).into(),
                                    controls,
                                    widget::divider::horizontal::default().into(),
                                    label_spin.into(),
                                ])
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

            Message::Surface(a) => {
                return cosmic::task::message(cosmic::Action::Cosmic(
                    cosmic::app::Action::Surface(a),
                ));
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

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }
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

//! The browser entry points: one `iced::daemon`, one window per live embed.
//!
//! `iced_winit` places each window in the DOM element named by
//! `PlatformSpecific::target` and creates the wgpu compositor once, so every
//! embed on a page shares one device and one render loop. The loader calls
//! [`open_scene`] and [`close_scene`] as figures enter and leave the viewport;
//! those run outside the daemon, so they push onto a queue that a subscription
//! drains.
//!
//! The runtime drops the compositor - and with it the wgpu device and queue -
//! whenever its last window closes, and creates a fresh one on the next open.
//! Firefox's WebGPU crashes on that churn (it keeps using the freed queue), so
//! the loader opens one [`keep_alive`] window first: an empty scene in a
//! one-pixel element off screen that is never closed, so the device lives as
//! long as the page.
//!
//! [`watch_messages`] taps one embed: every message its scene receives is also
//! reported to the page as a DOM event, which the project site prints.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use demo_common::{Scene, SceneMessage, page_theme};
use iced::futures::channel::mpsc;
use iced::futures::{SinkExt, Stream, StreamExt};
use iced::window::settings::PlatformSpecific;
use iced::{Element, Subscription, Task, Theme, window};
use wasm_bindgen::prelude::*;

use crate::SCENES;

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

/// Starts the daemon. Later calls are ignored: one runtime serves every embed.
#[wasm_bindgen]
pub fn run_gallery() {
    static STARTED: AtomicBool = AtomicBool::new(false);

    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }

    let _ = iced::daemon(Gallery::boot, Gallery::update, Gallery::view)
        .title("iced_nodegraph demos")
        .theme(Gallery::theme)
        .subscription(Gallery::subscription)
        .run();
}

/// Opens `scene` in the DOM element with id `target`, replacing it with a
/// canvas, on the page theme `theme` names (see `demo_common::PAGE_THEMES`).
#[wasm_bindgen]
pub fn open_scene(target: &str, scene: &str, theme: &str) {
    push(Command::Open {
        target: target.to_owned(),
        scene: scene.to_owned(),
        theme: theme.to_owned(),
    });
}

/// Closes the scene opened for `target`.
#[wasm_bindgen]
pub fn close_scene(target: &str) {
    push(Command::Close {
        target: target.to_owned(),
    });
}

/// Opens an empty, never-closed window in the DOM element with id `target`,
/// so the compositor outlives every embed.
#[wasm_bindgen]
pub fn keep_alive(target: &str) {
    push(Command::KeepAlive {
        target: target.to_owned(),
    });
}

/// Switches every live scene onto the page theme `theme` names. A name that is
/// not a page theme leaves every scene as it is.
#[wasm_bindgen]
pub fn set_theme(theme: &str) {
    push(Command::Theme {
        name: theme.to_owned(),
    });
}

/// Reports every message the scene at `target` receives, until that scene
/// closes, as a `gallery-message` `CustomEvent` on `window` whose `detail` is
/// `{ target, message }`, `message` being the demo message's `Debug` form.
///
/// Queued like the other commands, so it may precede the matching
/// [`open_scene`]; a later scene in the same target needs a fresh call.
#[wasm_bindgen]
pub fn watch_messages(target: &str) {
    push(Command::Watch {
        target: target.to_owned(),
    });
}

#[derive(Clone, Debug)]
enum Command {
    Open {
        target: String,
        scene: String,
        theme: String,
    },
    Close {
        target: String,
    },
    Theme {
        name: String,
    },
    KeepAlive {
        target: String,
    },
    Watch {
        target: String,
    },
}

/// Commands that arrived before the daemon's subscription was listening.
struct Queue {
    pending: VecDeque<Command>,
    wake: Option<mpsc::UnboundedSender<()>>,
}

static COMMANDS: Mutex<Queue> = Mutex::new(Queue {
    pending: VecDeque::new(),
    wake: None,
});

fn push(command: Command) {
    let mut queue = COMMANDS.lock().expect("command queue poisoned");

    queue.pending.push_back(command);

    if let Some(wake) = &queue.wake {
        let _ = wake.unbounded_send(());
    }
}

fn command_stream() -> impl Stream<Item = Message> {
    iced::stream::channel(1, |mut output: mpsc::Sender<Message>| async move {
        let (wake, mut woken) = mpsc::unbounded();
        COMMANDS
            .lock()
            .expect("command queue poisoned")
            .wake
            .replace(wake);

        loop {
            let pending: Vec<Command> = COMMANDS
                .lock()
                .expect("command queue poisoned")
                .pending
                .drain(..)
                .collect();

            for command in pending {
                if output.send(Message::Command(command)).await.is_err() {
                    return;
                }
            }

            if woken.next().await.is_none() {
                return;
            }
        }
    })
}

struct Live {
    target: String,
    scene: Box<dyn Scene>,
}

struct Gallery {
    live: HashMap<window::Id, Live>,
    /// Targets whose scene messages are reported to the page. A target stays
    /// here until its window closes, so a watch may precede its scene.
    watched: HashSet<String>,
}

#[derive(Clone, Debug)]
enum Message {
    Command(Command),
    Scene(window::Id, SceneMessage),
    Closed(window::Id),
}

impl Gallery {
    fn boot() -> (Self, Task<Message>) {
        (
            Self {
                live: HashMap::new(),
                watched: HashSet::new(),
            },
            Task::none(),
        )
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Command(Command::Open {
                target,
                scene,
                theme,
            }) => {
                let Some(def) = SCENES.iter().find(|def| def.name == scene) else {
                    return Task::none();
                };

                if self.live.values().any(|live| live.target == target) {
                    return Task::none();
                }

                let (mut scene, boot) = (def.boot)();
                if let Some(theme) = page_theme(&theme) {
                    scene.set_theme(theme);
                }
                let (id, open) = window::open(window::Settings {
                    platform_specific: PlatformSpecific {
                        target: Some(target.clone()),
                    },
                    ..Default::default()
                });

                self.live.insert(id, Live { target, scene });

                Task::batch([
                    open.discard(),
                    boot.map(move |message| Message::Scene(id, message)),
                ])
            }
            Message::Command(Command::Theme { name }) => {
                if let Some(theme) = page_theme(&name) {
                    for live in self.live.values_mut() {
                        live.scene.set_theme(theme.clone());
                    }
                }

                Task::none()
            }
            Message::Command(Command::KeepAlive { target }) => {
                // No `Live` entry: `view` draws nothing and `theme` answers
                // `Dark` for a window without a scene, and `Close` cannot name
                // it because it never had a scene.
                let (_, open) = window::open(window::Settings {
                    platform_specific: PlatformSpecific {
                        target: Some(target),
                    },
                    ..Default::default()
                });

                open.discard()
            }
            Message::Command(Command::Close { target }) => {
                let closing = self
                    .live
                    .iter()
                    .find(|(_, live)| live.target == target)
                    .map(|(id, _)| *id);

                match closing {
                    Some(id) => window::close(id),
                    None => Task::none(),
                }
            }
            Message::Command(Command::Watch { target }) => {
                let _ = self.watched.insert(target);

                Task::none()
            }
            Message::Scene(id, scene_message) => match self.live.get_mut(&id) {
                Some(live) => {
                    if self.watched.contains(&live.target) {
                        report(&live.target, &scene_message);
                    }

                    live.scene
                        .update(scene_message)
                        .map(move |message| Message::Scene(id, message))
                }
                None => Task::none(),
            },
            Message::Closed(id) => {
                if let Some(live) = self.live.remove(&id) {
                    let _ = self.watched.remove(&live.target);
                }

                Task::none()
            }
        }
    }

    fn view(&self, id: window::Id) -> Element<'_, Message> {
        match self.live.get(&id) {
            Some(live) => live
                .scene
                .view()
                .map(move |message| Message::Scene(id, message)),
            None => iced::widget::column![].into(),
        }
    }

    fn theme(&self, id: window::Id) -> Theme {
        // A window without a scene exists only between a close request and the
        // window actually going away; `iced::Theme` has no `Default`, so name
        // the one that matches the embed frame.
        self.live
            .get(&id)
            .map_or(Theme::Dark, |live| live.scene.theme())
    }

    fn subscription(&self) -> Subscription<Message> {
        // `with` keys each scene's subscription by window id, which both routes
        // the message back and keeps the identities distinct; `map` must stay
        // non-capturing to pass iced's zero-size check.
        let scenes = self.live.iter().map(|(id, live)| {
            live.scene
                .subscription()
                .with(*id)
                .map(|(id, message)| Message::Scene(id, message))
        });

        Subscription::batch(
            [
                window::close_events().map(Message::Closed),
                Subscription::run(command_stream),
            ]
            .into_iter()
            .chain(scenes),
        )
    }
}

/// Dispatches `message` as a `gallery-message` event on `window`. The tap is
/// an observer: a page without a `window` or a failed dispatch leaves the
/// scene untouched.
fn report(target: &str, message: &SceneMessage) {
    let Some(window) = web_sys::window() else {
        return;
    };

    let detail = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&detail, &"target".into(), &target.into());
    let _ = js_sys::Reflect::set(&detail, &"message".into(), &format!("{message:?}").into());

    let init = web_sys::CustomEventInit::new();
    init.set_detail(&detail);

    if let Ok(event) = web_sys::CustomEvent::new_with_event_init_dict("gallery-message", &init) {
        let _ = window.dispatch_event(&event);
    }
}

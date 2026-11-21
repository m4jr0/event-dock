use chrono::{DateTime, Utc};
use eframe::egui;
use eventdock::{EventStore, IncomingEvent, LogLevel, Timestamp};
use std::{
    error::Error,
    io::{self, BufRead, BufReader},
    net::{TcpListener, TcpStream},
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};

type AppResult<T> = Result<T, Box<dyn Error + Send + Sync>>;
const DEFAULT_ADDRESS: &str = "127.0.0.1:7878";
const STORE_CAPACITY: usize = 100_000;
const MAX_VISIBLE_EVENTS: usize = 1_000;

fn main() -> eframe::Result {
    let (event_sender, event_receiver) = mpsc::channel();
    spawn_listener(DEFAULT_ADDRESS.to_owned(), event_sender);
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "EventDock",
        options,
        Box::new(move |_creation_context| {
            Ok(Box::new(EventDockApp::new(DEFAULT_ADDRESS, event_receiver)))
        }),
    )
}

struct EventDockApp {
    address: String,
    receiver: Receiver<CollectorMessage>,
    store: EventStore,
    status: CollectorStatus,
    minimum_level: LogLevel,
    search_text: String,
}

impl EventDockApp {
    fn new(
        address: impl Into<String>,
        receiver: Receiver<CollectorMessage>,
    ) -> Self {
        Self {
            address: address.into(),
            receiver,
            store: EventStore::new(STORE_CAPACITY),
            status: CollectorStatus::Starting,
            minimum_level: LogLevel::Trace,
            search_text: String::new(),
        }
    }

    fn receive_pending_messages(&mut self) {
        while let Ok(message) = self.receiver.try_recv() {
            match message {
                CollectorMessage::Event(event) => {
                    self.store.push(event);
                }
                CollectorMessage::Listening => {
                    self.status = CollectorStatus::Listening;
                }
                CollectorMessage::ClientConnected => {
                    self.status = CollectorStatus::ClientConnected;
                }
                CollectorMessage::ClientDisconnected => {
                    self.status = CollectorStatus::Listening;
                }
                CollectorMessage::Error(error) => {
                    self.status = CollectorStatus::Error(error);
                }
            }
        }
    }

    fn event_matches_filters(
        &self,
        event: &IncomingEvent,
    ) -> bool {
        if event.level < self.minimum_level {
            return false;
        }
        if self.search_text.is_empty() {
            return true;
        }
        let search = self.search_text.to_lowercase();
        event
            .message
            .to_lowercase()
            .contains(&search)
            || event
                .source
                .to_lowercase()
                .contains(&search)
    }

    fn visible_event_count(&self) -> usize {
        self.store
            .iter()
            .filter(|stored| self.event_matches_filters(&stored.event))
            .count()
    }
}

impl eframe::App for EventDockApp {
    fn ui(
        &mut self,
        root_ui: &mut egui::Ui,
        _frame: &mut eframe::Frame,
    ) {
        self.receive_pending_messages();
        egui::Panel::top("toolbar").show(root_ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("EventDock");
                ui.separator();
                ui.label(format!("Listening on {}", self.address));
                ui.separator();
                match &self.status {
                    CollectorStatus::Starting => {
                        ui.label("Starting…");
                    }
                    CollectorStatus::Listening => {
                        ui.label("Listening");
                    }
                    CollectorStatus::ClientConnected => {
                        ui.label("Client connected");
                    }
                    CollectorStatus::Error(error) => {
                        ui.colored_label(
                            ui.visuals().error_fg_color,
                            format!("Error: {error}"),
                        );
                    }
                }
            });
        });
        egui::Panel::left("filters")
            .resizable(false)
            .default_size(220.0)
            .show(root_ui, |ui| {
                ui.heading("Filters");
                ui.add_space(8.0);
                ui.label("Minimum level");
                egui::ComboBox::from_id_salt("minimum_log_level")
                    .selected_text(self.minimum_level.to_string())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.minimum_level,
                            LogLevel::Trace,
                            "trace",
                        );
                        ui.selectable_value(
                            &mut self.minimum_level,
                            LogLevel::Debug,
                            "debug",
                        );
                        ui.selectable_value(
                            &mut self.minimum_level,
                            LogLevel::Info,
                            "info",
                        );
                        ui.selectable_value(
                            &mut self.minimum_level,
                            LogLevel::Warning,
                            "warning",
                        );
                        ui.selectable_value(
                            &mut self.minimum_level,
                            LogLevel::Error,
                            "error",
                        );
                        ui.selectable_value(
                            &mut self.minimum_level,
                            LogLevel::Critical,
                            "critical",
                        );
                    });
                ui.add_space(12.0);
                ui.label("Text search");
                ui.text_edit_singleline(&mut self.search_text);
                ui.add_space(12.0);
                ui.separator();
                ui.add_space(12.0);
                ui.label(format!("Stored: {}", self.store.len()));
                ui.label(format!("Visible: {}", self.visible_event_count()));
                ui.label(format!("Capacity: {}", self.store.capacity()));
                ui.add_space(12.0);
                if ui.button("Clear filters").clicked() {
                    self.minimum_level = LogLevel::Trace;
                    self.search_text.clear();
                }
            });
        egui::CentralPanel::default().show(root_ui, |ui| {
            ui.heading("Events");
            ui.add_space(8.0);
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .stick_to_bottom(false)
                .show(ui, |ui| {
                    egui::Grid::new("event_grid")
                        .num_columns(5)
                        .striped(true)
                        .spacing([16.0, 6.0])
                        .show(ui, |ui| {
                            ui.strong("ID");
                            ui.strong("Time (UTC)");
                            ui.strong("Level");
                            ui.strong("Source");
                            ui.strong("Message");
                            ui.end_row();
                            for stored in self
                                .store
                                .iter()
                                .rev()
                                .filter(|stored| {
                                    self.event_matches_filters(&stored.event)
                                })
                                .take(MAX_VISIBLE_EVENTS)
                            {
                                let event = &stored.event;
                                ui.label(stored.id.to_string());
                                ui.label(format_timestamp(event.timestamp));
                                show_level(ui, event.level);
                                ui.label(&event.source);
                                ui.label(&event.message);
                                ui.end_row();
                            }
                        });
                });
        });
        root_ui
            .ctx()
            .request_repaint_after(Duration::from_millis(50));
    }
}

fn format_timestamp(timestamp: Timestamp) -> String {
    let nanos = timestamp.as_unix_nanos();
    let seconds = nanos.div_euclid(1_000_000_000);
    let subsec_nanos = nanos.rem_euclid(1_000_000_000) as u32;
    match DateTime::<Utc>::from_timestamp(seconds, subsec_nanos) {
        Some(datetime) => datetime
            .format("%H:%M:%S%.9f")
            .to_string(),
        None => format!("{nanos} ns"),
    }
}

#[derive(Debug)]
enum CollectorMessage {
    Event(IncomingEvent),
    Listening,
    ClientConnected,
    ClientDisconnected,
    Error(String),
}

enum CollectorStatus {
    Starting,
    Listening,
    ClientConnected,
    Error(String),
}

fn spawn_listener(
    address: String,
    sender: Sender<CollectorMessage>,
) {
    thread::spawn(move || {
        if let Err(error) = run_listener(&address, &sender) {
            let _ = sender.send(CollectorMessage::Error(error.to_string()));
        }
    });
}

fn run_listener(
    address: &str,
    sender: &Sender<CollectorMessage>,
) -> AppResult<()> {
    let listener = TcpListener::bind(address)?;
    sender
        .send(CollectorMessage::Listening)
        .map_err(|_| {
            io::Error::new(
                io::ErrorKind::BrokenPipe,
                "GUI stopped receiving collector messages",
            )
        })?;
    for connection in listener.incoming() {
        match connection {
            Ok(stream) => {
                let client_sender = sender.clone();
                thread::spawn(move || {
                    let _ =
                        client_sender.send(CollectorMessage::ClientConnected);
                    if let Err(error) = handle_client(stream, &client_sender) {
                        let _ = client_sender
                            .send(CollectorMessage::Error(error.to_string()));
                    }
                    let _ = client_sender
                        .send(CollectorMessage::ClientDisconnected);
                });
            }
            Err(error) => {
                sender
                    .send(CollectorMessage::Error(format!(
                        "failed to accept client: {error}"
                    )))
                    .map_err(|_| {
                        io::Error::new(
                            io::ErrorKind::BrokenPipe,
                            "GUI stopped receiving collector messages",
                        )
                    })?;
            }
        }
    }
    Ok(())
}

fn handle_client(
    stream: TcpStream,
    sender: &Sender<CollectorMessage>,
) -> AppResult<()> {
    let reader = BufReader::new(stream);
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let event: IncomingEvent = serde_json::from_str(&line)?;
        sender
            .send(CollectorMessage::Event(event))
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "GUI stopped receiving events",
                )
            })?;
    }
    Ok(())
}

fn show_level(
    ui: &mut egui::Ui,
    level: LogLevel,
) {
    let color = match level {
        LogLevel::Trace => ui.visuals().weak_text_color(),
        LogLevel::Debug => ui.visuals().text_color(),
        LogLevel::Info => ui.visuals().text_color(),
        LogLevel::Warning => egui::Color32::from_rgb(220, 170, 40),
        LogLevel::Error => ui.visuals().error_fg_color,
        LogLevel::Critical => egui::Color32::from_rgb(255, 70, 90),
    };
    ui.colored_label(color, level.to_string());
}

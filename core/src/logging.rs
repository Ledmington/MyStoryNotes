use std::sync::{Arc, Mutex};
use std::time::Instant;

use log::{Level, LevelFilter, Log, Metadata, Record};
use time::OffsetDateTime;
use time::format_description::FormatItem;
use time::macros::format_description;

/// `2026-08-10 20:15:03.123` — UTC, since the local offset can't be read soundly from a
/// multi-threaded program.
const TIMESTAMP_FORMAT: &[FormatItem<'_>] =
    format_description!("[year]-[month]-[day] [hour]:[minute]:[second].[subsecond digits:3]");

/// A queued error-level message together with when it was queued, so the UI can time its
/// auto-dismissal and fade-out animation.
#[derive(Clone)]
pub struct Notification {
    pub message: String,
    pub created_at: Instant,
}

/// Error-level messages, queued for the UI to show as dismissible red popups. Cheap to clone;
/// clones share the same underlying queue.
#[derive(Clone, Default)]
pub struct Notifications(Arc<Mutex<Vec<Notification>>>);

impl Notifications {
    pub fn snapshot(&self) -> Vec<Notification> {
        self.0.lock().unwrap().clone()
    }

    pub fn dismiss(&self, index: usize) {
        let mut messages = self.0.lock().unwrap();

        if index < messages.len() {
            messages.remove(index);
        }
    }

    fn push(&self, message: String) {
        self.0.lock().unwrap().push(Notification {
            message,
            created_at: Instant::now(),
        });
    }
}

struct Logger {
    notifications: Notifications,
}

impl Log for Logger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        // Debug-level verbosity is only interesting for our own code; third-party crates
        // (winit, wgpu, ...) also log through this same facade and would otherwise flood
        // stderr with per-frame windowing/rendering chatter. Checked against a literal prefix
        // rather than `env!("CARGO_PKG_NAME")`: that macro expands to *this* crate's own name
        // (`my_story_notes_core`) wherever this file happens to be compiled, but log targets
        // from the GUI crate (`my_story_notes::...`) need to count as "our own code" too — which
        // a plain `"my_story_notes"` prefix catches for both, since `my_story_notes_core` also
        // starts with it by construction.
        let level_cap = if metadata.target().starts_with("my_story_notes") {
            max_level()
        } else {
            LevelFilter::Warn
        };

        metadata.level() <= level_cap
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let timestamp = OffsetDateTime::now_utc()
            .format(TIMESTAMP_FORMAT)
            .unwrap_or_else(|_| "?".to_owned());

        eprintln!(
            "[{timestamp}] [{}] {}: {}",
            record.level(),
            record.target(),
            record.args()
        );

        if record.level() == Level::Error {
            self.notifications.push(record.args().to_string());
        }
    }

    fn flush(&self) {}
}

fn max_level() -> LevelFilter {
    if cfg!(debug_assertions) {
        LevelFilter::Debug
    } else {
        LevelFilter::Info
    }
}

/// Installs the app's logger: info/warning/error messages always print to stderr, debug
/// messages print only in debug builds, and error-level messages are additionally queued for
/// the bottom-right notification popups. Returns the queue for the UI to read from.
pub fn init() -> Notifications {
    let notifications = Notifications::default();

    log::set_boxed_logger(Box::new(Logger {
        notifications: notifications.clone(),
    }))
    .expect("logger should only be installed once");
    log::set_max_level(max_level());

    notifications
}

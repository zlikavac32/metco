mod console;
mod postgresql;

use crate::metrics::TimeFrame;
use chrono::{DateTime, Utc};

pub trait Backend {
    fn publish(&mut self, time: &DateTime<Utc>, time_frame: &TimeFrame, logger: Logger);
}

pub struct Logger {
    backend: String,
}

impl Logger {
    pub fn new(backend: String) -> Self {
        Logger { backend }
    }

    pub fn debug(&self, msg: &str) {
        log::debug!("[{}] {}", self.backend, msg);
    }

    pub fn info(&self, msg: &str) {
        log::info!("[{}] {}", self.backend, msg);
    }
}

pub use console::Console;
pub use postgresql::PostgreSQL;

mod console;
mod elastic;
mod postgresql;

use crate::metrics::TimeFrame;
use chrono::{DateTime, Utc};
use std::fmt::Display;

pub use console::Console;
pub use elastic::ElasticSearch;
pub use postgresql::PostgreSQL;

pub trait Backend {
    fn publish(&mut self, time: &DateTime<Utc>, time_frame: &TimeFrame, logger: &Logger);
}

/// Logger that prefixes every message with the backend name.
#[derive(Debug, Clone, Copy)]
pub struct Logger<'a> {
    backend: &'a str,
}

impl<'a> Logger<'a> {
    pub fn new(backend: &'a str) -> Self {
        Self { backend }
    }

    pub fn debug(&self, msg: impl Display) {
        log::debug!("[{}] {}", self.backend, msg);
    }

    pub fn info(&self, msg: impl Display) {
        log::info!("[{}] {}", self.backend, msg);
    }

    pub fn error(&self, msg: impl Display) {
        log::error!("[{}] {}", self.backend, msg);
    }
}

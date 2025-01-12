mod console;
mod postgresql;

use crate::metrics::TimeFrame;
use chrono::{DateTime, Utc};

pub trait Backend {
    fn publish(&mut self, time: &DateTime<Utc>, time_frame: &TimeFrame);
}

pub use console::Console;
pub use postgresql::PostgreSQL;

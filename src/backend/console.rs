use crate::backend::{Backend, Logger};
use crate::metrics::{Identifier, Statistics, TimeFrame};
use chrono::{DateTime, Utc};

#[derive(Debug, Default)]
pub struct Console;

impl Console {
    fn log_identifier(identifier: &Identifier, logger: &Logger) {
        logger.info(format_args!(
            "  {} ({:?})",
            identifier.name(),
            identifier.tags()
        ));
    }

    fn log_statistics(statistics: &Statistics, logger: &Logger) {
        for (name, value) in statistics.summary() {
            logger.info(format_args!("    {name}: {value}"));
        }
    }
}

impl Backend for Console {
    fn publish(&mut self, time: &DateTime<Utc>, time_frame: &TimeFrame, logger: &Logger) {
        logger.info(time.to_rfc3339());

        if !time_frame.gauges().is_empty() {
            logger.info("Gauges:");

            for (identifier, gauge) in time_frame.gauges() {
                Self::log_identifier(identifier, logger);
                logger.info(format_args!("    current: {}", gauge.current));
                Self::log_statistics(&gauge.statistics, logger);
            }
        }

        if !time_frame.counters().is_empty() {
            logger.info("Counters:");

            for (identifier, value) in time_frame.counters() {
                logger.info(format_args!(
                    "  {} ({:?}) - {value}",
                    identifier.name(),
                    identifier.tags()
                ));
            }
        }

        if !time_frame.histograms().is_empty() {
            logger.info("Histograms:");

            for (identifier, statistics) in time_frame.histograms() {
                Self::log_identifier(identifier, logger);
                Self::log_statistics(statistics, logger);
            }
        }

        if !time_frame.timings().is_empty() {
            logger.info("Timings:");

            for (identifier, statistics) in time_frame.timings() {
                Self::log_identifier(identifier, logger);
                Self::log_statistics(statistics, logger);
            }
        }
    }
}

use crate::backend::{Backend, Logger};
use crate::metrics::TimeFrame;
use chrono::{DateTime, Utc};

#[derive(Debug, Default)]
pub struct Console {}

impl Backend for Console {
    fn publish(&mut self, time: &DateTime<Utc>, time_frame: &TimeFrame, logger: Logger) {
        logger.info(&format!(
            "{} - host: {}",
            time.to_rfc3339(),
            time_frame.host()
        ));

        if !time_frame.gauges().is_empty() {
            logger.info("Gauges:");

            time_frame
                .gauges()
                .iter()
                .for_each(|(name, value)| logger.info(&format!("  {name} - {value}")));
        }

        if !time_frame.counters().is_empty() {
            logger.info("Counters:");

            time_frame.counters().iter().for_each(|(name, stats)| {
                logger.info(&format!("  {name}"));
                logger.info(&format!("    count: {}", stats.count()));
                logger.info(&format!("    sum: {}", stats.sum()));
                logger.info(&format!("    std: {}", stats.std()));
                logger.info(&format!("    median: {}", stats.median()));
                logger.info(&format!("    p75: {}", stats.percentile(0.75)));
                logger.info(&format!("    p90: {}", stats.percentile(0.90)));

                if let Some(min) = stats.min() {
                    logger.info(&format!("    min: {min}"));
                }

                if let Some(max) = stats.max() {
                    logger.info(&format!("    max: {max}"));
                }
            });
        }

        if !time_frame.timings().is_empty() {
            logger.info("Timings:");

            time_frame.timings().iter().for_each(|(name, stats)| {
                logger.info(&format!("  {name}"));
                logger.info(&format!("    count: {}", stats.count()));
                logger.info(&format!("    sum: {}", stats.sum()));
                logger.info(&format!("    std: {}", stats.std()));
                logger.info(&format!("    median: {}", stats.median()));
                logger.info(&format!("    p75: {}", stats.percentile(0.75)));
                logger.info(&format!("    p90: {}", stats.percentile(0.90)));

                if let Some(min) = stats.min() {
                    logger.info(&format!("    min: {min}"));
                }

                if let Some(max) = stats.max() {
                    logger.info(&format!("    max: {max}"));
                }
            });
        }
    }
}

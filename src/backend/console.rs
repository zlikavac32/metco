use crate::backend::{Backend, Logger};
use crate::metrics::TimeFrame;
use chrono::{DateTime, Utc};

#[derive(Debug, Default)]
pub struct Console {}

impl Backend for Console {
    fn publish(&mut self, time: &DateTime<Utc>, time_frame: &TimeFrame, logger: Logger) {
        logger.info(time.to_rfc3339().as_str());

        if !time_frame.gauges().is_empty() {
            logger.info("Gauges:");

            time_frame
                .gauges()
                .iter()
                .for_each(|(identifier, (current, stats))| {
                    logger.info(&format!(
                        "  {} ({:?})",
                        identifier.name(),
                        identifier.tags()
                    ));
                    logger.info(&format!("    current: {}", current));
                    logger.info(&format!("    count: {}", stats.count()));
                    logger.info(&format!("    sum: {}", stats.sum()));
                    logger.info(&format!("    std: {}", stats.std()));
                    logger.info(&format!("    median: {}", stats.median()));
                    logger.info(&format!("    p75: {}", stats.percentile(75.into())));
                    logger.info(&format!("    p90: {}", stats.percentile(90.into())));
                    logger.info(&format!("    p99: {}", stats.percentile(99.into())));
                    logger.info(&format!("    min: {}", stats.min()));
                    logger.info(&format!("    max: {}", stats.max()));
                });
        }

        if !time_frame.counters().is_empty() {
            logger.info("Counters:");

            time_frame
                .counters()
                .iter()
                .for_each(|(identifier, value)| {
                    logger.info(&format!(
                        "  {} ({:?}) - {value}",
                        identifier.name(),
                        identifier.tags()
                    ))
                });
        }

        if !time_frame.histograms().is_empty() {
            logger.info("Histograms:");

            time_frame
                .histograms()
                .iter()
                .for_each(|(identifier, stats)| {
                    logger.info(&format!(
                        "  {} ({:?})",
                        identifier.name(),
                        identifier.tags()
                    ));
                    logger.info(&format!("    count: {}", stats.count()));
                    logger.info(&format!("    sum: {}", stats.sum()));
                    logger.info(&format!("    std: {}", stats.std()));
                    logger.info(&format!("    median: {}", stats.median()));
                    logger.info(&format!("    p75: {}", stats.percentile(75.into())));
                    logger.info(&format!("    p90: {}", stats.percentile(90.into())));
                    logger.info(&format!("    p99: {}", stats.percentile(99.into())));
                    logger.info(&format!("    min: {}", stats.min()));
                    logger.info(&format!("    max: {}", stats.max()));
                });
        }

        if !time_frame.timings().is_empty() {
            logger.info("Timings:");

            time_frame.timings().iter().for_each(|(identifier, stats)| {
                logger.info(&format!(
                    "  {} ({:?})",
                    identifier.name(),
                    identifier.tags()
                ));
                logger.info(&format!("    count: {}", stats.count()));
                logger.info(&format!("    sum: {}", stats.sum()));
                logger.info(&format!("    std: {}", stats.std()));
                logger.info(&format!("    median: {}", stats.median()));
                logger.info(&format!("    p75: {}", stats.percentile(75.into())));
                logger.info(&format!("    p90: {}", stats.percentile(90.into())));
                logger.info(&format!("    p99: {}", stats.percentile(99.into())));
                logger.info(&format!("    min: {}", stats.min()));
                logger.info(&format!("    max: {}", stats.max()));
            });
        }
    }
}

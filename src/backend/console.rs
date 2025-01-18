use crate::backend::Backend;
use crate::metrics::TimeFrame;
use chrono::{DateTime, Utc};

#[derive(Debug, Default)]
pub struct Console {}

impl Backend for Console {
    fn publish(&mut self, time: &DateTime<Utc>, time_frame: &TimeFrame) {
        println!("{} - host: {}", time.to_rfc3339(), time_frame.host);

        if !time_frame.gauges.is_empty() {
            println!("Gauges:");

            time_frame
                .gauges
                .iter()
                .for_each(|(name, value)| println!("  {name} - {value}"));
        }

        if !time_frame.counters.is_empty() {
            println!("Counters:");

            time_frame.counters.iter().for_each(|(name, stats)| {
                println!("  {name}");
                println!("    count: {}", stats.count());
                println!("    sum: {}", stats.sum());
                println!("    avg: {}", stats.average());
                println!("    std: {}", stats.std());
                println!("    median: {}", stats.median());
                println!("    p75: {}", stats.percentile(0.75));
                println!("    p90: {}", stats.percentile(0.90));
            });
        }

        if !time_frame.timings.is_empty() {
            println!("Timings:");

            time_frame.timings.iter().for_each(|(name, stats)| {
                println!("  {name}");
                println!("    count: {}", stats.count());
                println!("    sum: {}", stats.sum());
                println!("    avg: {}", stats.average());
                println!("    std: {}", stats.std());
                println!("    median: {}", stats.median());
                println!("    p75: {}", stats.percentile(0.75));
                println!("    p90: {}", stats.percentile(0.90));
            });
        }
    }
}

use crate::metrics::TimeFrame;
use chrono::{DateTime, Utc};
use postgres::types::ToSql;
use std::fmt::{Debug, Formatter};

pub trait Backend {
    fn publish(&mut self, time: &DateTime<Utc>, time_frame: &TimeFrame);
}

#[derive(Debug, Default)]
pub struct Console {}

impl Backend for Console {
    fn publish(&mut self, time: &DateTime<Utc>, time_frame: &TimeFrame) {
        println!("{}", time.to_rfc3339());

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

/*
create type metric_kind as enum ('gauge', 'counter', 'timing');

create table metrics
(
    name  text        not null,
    kind  metric_kind not null,
    time  timestamptz not null,
    value float8,
    primary key (name, kind, time)
);
 */

pub struct PostgreSQL {
    client: postgres::Client,
}

impl Debug for PostgreSQL {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("PostgreSQL {}")
    }
}

#[derive(Debug, ToSql)]
#[postgres(name = "metric_kind")]
enum MetricKind {
    #[postgres(name = "gauge")]
    Gauge,
    #[postgres(name = "counter")]
    Counter,
    #[postgres(name = "timing")]
    Timing,
}

impl PostgreSQL {
    pub fn new(client: postgres::Client) -> Self {
        Self { client }
    }

    fn insert(&mut self, time: &DateTime<Utc>, metric_kind: MetricKind, name: &str, value: f64) {
        let sql = r"
insert into metrics (name, kind, time, value)
values ($1, $2, $3, $4)
on conflict (name, kind, time)
    do nothing
";

        if let Err(err) = self
            .client
            .execute(sql, &[&name, &metric_kind, time, &value])
        {
            log::error!("{err}");
        }
    }
}

impl Backend for PostgreSQL {
    fn publish(&mut self, time: &DateTime<Utc>, time_frame: &TimeFrame) {
        time_frame
            .gauges
            .iter()
            .for_each(|(name, value)| self.insert(time, MetricKind::Gauge, name, *value as f64));

        time_frame.counters.iter().for_each(|(name, stats)| {
            self.insert(
                time,
                MetricKind::Counter,
                &format!("{name}.count"),
                stats.count() as f64,
            );
            self.insert(
                time,
                MetricKind::Counter,
                &format!("{name}.sum"),
                stats.sum() as f64,
            );
            self.insert(
                time,
                MetricKind::Counter,
                &format!("{name}.avg"),
                stats.average(),
            );
            self.insert(
                time,
                MetricKind::Counter,
                &format!("{name}.std"),
                stats.std(),
            );
            self.insert(
                time,
                MetricKind::Counter,
                &format!("{name}.median"),
                stats.median(),
            );
            self.insert(
                time,
                MetricKind::Counter,
                &format!("{name}.p75"),
                stats.percentile(0.75) as f64,
            );
            self.insert(
                time,
                MetricKind::Counter,
                &format!("{name}.p90"),
                stats.percentile(0.90) as f64,
            );
        });

        time_frame.timings.iter().for_each(|(name, stats)| {
            self.insert(
                time,
                MetricKind::Timing,
                &format!("{name}.count"),
                stats.count() as f64,
            );
            self.insert(
                time,
                MetricKind::Timing,
                &format!("{name}.sum"),
                stats.sum() as f64,
            );
            self.insert(
                time,
                MetricKind::Timing,
                &format!("{name}.avg"),
                stats.average(),
            );
            self.insert(
                time,
                MetricKind::Timing,
                &format!("{name}.std"),
                stats.std(),
            );
            self.insert(
                time,
                MetricKind::Timing,
                &format!("{name}.median"),
                stats.median(),
            );
            self.insert(
                time,
                MetricKind::Timing,
                &format!("{name}.p75"),
                stats.percentile(0.75) as f64,
            );
            self.insert(
                time,
                MetricKind::Timing,
                &format!("{name}.p90"),
                stats.percentile(0.90) as f64,
            );
        });
    }
}

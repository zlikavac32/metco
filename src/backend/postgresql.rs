use crate::backend::{Backend, Logger};
use crate::metrics::TimeFrame;
use chrono::{DateTime, Utc};
use std::fmt::{Debug, Formatter};

/*
Used table structure is bellow.

create table metric_counters
(
    name  text        not null,
    time  timestamptz not null,
    host  text        not null,
    value float8,
    primary key (name, time, host)
);

create table metric_timers
(
    name  text        not null,
    time  timestamptz not null,
    host  text        not null,
    value float8,
    primary key (name, time, host)
);

create table metric_gauges
(
    name  text        not null,
    time  timestamptz not null,
    host  text        not null,
    value float8,
    primary key (name, time, host)
);

It's also possible to use https://github.com/timescale/timescaledb and
slightly modify create commands above to use hyper-tables.

create table metric_counters
(
    name  text        not null,
    time  timestamptz not null,
    host  text        not null,
    value float8,
    primary key (name, time, host)
)
with (
  timescaledb.hypertable,
  timescaledb.partition_column='time',
  timescaledb.segmentby='name'
);

create table metric_timers
(
    name  text        not null,
    time  timestamptz not null,
    host  text        not null,
    value float8,
    primary key (name, time, host)
)
with (
  timescaledb.hypertable,
  timescaledb.partition_column='time',
  timescaledb.segmentby='name'
);

create table metric_gauges
(
    name  text        not null,
    time  timestamptz not null,
    host  text        not null,
    value float8,
    primary key (name, time, host)
)
with (
  timescaledb.hypertable,
  timescaledb.partition_column='time',
  timescaledb.segmentby='name'
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

#[derive(Debug)]
enum MetricKind {
    Gauge,
    Counter,
    Timer,
}

impl PostgreSQL {
    pub fn new(client: postgres::Client) -> Self {
        Self { client }
    }

    fn insert(
        &mut self,
        time: &DateTime<Utc>,
        host: &str,
        metric_kind: MetricKind,
        name: &str,
        value: f64,
        logger: &Logger,
    ) {
        let table_name = match metric_kind {
            MetricKind::Gauge => "metric_gauges",
            MetricKind::Counter => "metric_counters",
            MetricKind::Timer => "metric_timers",
        };

        let sql = format!(
            r"
insert into {table_name} (name, time, host, value)
values ($1, $2, $3, $4)
on conflict (name, time, host)
    do nothing
"
        );

        if let Err(err) = self.client.execute(&sql, &[&name, time, &host, &value]) {
            logger.error(&format!("Insert record failed: {err}"));
        }
    }
}

impl Backend for PostgreSQL {
    fn publish(&mut self, time: &DateTime<Utc>, time_frame: &TimeFrame, logger: Logger) {
        time_frame.gauges().iter().for_each(|(name, value)| {
            self.insert(
                time,
                time_frame.host(),
                MetricKind::Gauge,
                name,
                *value as f64,
                &logger,
            );

            logger.debug(&format!("Processed gauge {name}"));
        });

        time_frame.counters().iter().for_each(|(name, stats)| {
            self.insert(
                time,
                time_frame.host(),
                MetricKind::Counter,
                &format!("{name}.count"),
                stats.count() as f64,
                &logger,
            );
            self.insert(
                time,
                time_frame.host(),
                MetricKind::Counter,
                &format!("{name}.sum"),
                stats.sum() as f64,
                &logger,
            );
            self.insert(
                time,
                time_frame.host(),
                MetricKind::Counter,
                &format!("{name}.std"),
                stats.std(),
                &logger,
            );
            self.insert(
                time,
                time_frame.host(),
                MetricKind::Counter,
                &format!("{name}.median"),
                stats.median(),
                &logger,
            );
            self.insert(
                time,
                time_frame.host(),
                MetricKind::Counter,
                &format!("{name}.p75"),
                stats.percentile(0.75) as f64,
                &logger,
            );
            self.insert(
                time,
                time_frame.host(),
                MetricKind::Counter,
                &format!("{name}.p90"),
                stats.percentile(0.90) as f64,
                &logger,
            );

            if let Some(min) = stats.min() {
                self.insert(
                    time,
                    time_frame.host(),
                    MetricKind::Counter,
                    &format!("{name}.min"),
                    min as f64,
                    &logger,
                );
            }

            if let Some(max) = stats.max() {
                self.insert(
                    time,
                    time_frame.host(),
                    MetricKind::Counter,
                    &format!("{name}.max"),
                    max as f64,
                    &logger,
                );
            }

            logger.debug(&format!("Processed counter {name}"));
        });

        time_frame.timings().iter().for_each(|(name, stats)| {
            self.insert(
                time,
                time_frame.host(),
                MetricKind::Timer,
                &format!("{name}.count"),
                stats.count() as f64,
                &logger,
            );
            self.insert(
                time,
                time_frame.host(),
                MetricKind::Timer,
                &format!("{name}.sum"),
                stats.sum() as f64,
                &logger,
            );
            self.insert(
                time,
                time_frame.host(),
                MetricKind::Timer,
                &format!("{name}.std"),
                stats.std(),
                &logger,
            );
            self.insert(
                time,
                time_frame.host(),
                MetricKind::Timer,
                &format!("{name}.median"),
                stats.median(),
                &logger,
            );
            self.insert(
                time,
                time_frame.host(),
                MetricKind::Timer,
                &format!("{name}.p75"),
                stats.percentile(0.75) as f64,
                &logger,
            );
            self.insert(
                time,
                time_frame.host(),
                MetricKind::Timer,
                &format!("{name}.p90"),
                stats.percentile(0.90) as f64,
                &logger,
            );

            if let Some(min) = stats.min() {
                self.insert(
                    time,
                    time_frame.host(),
                    MetricKind::Timer,
                    &format!("{name}.min"),
                    min as f64,
                    &logger,
                );
            }

            if let Some(max) = stats.max() {
                self.insert(
                    time,
                    time_frame.host(),
                    MetricKind::Timer,
                    &format!("{name}.max"),
                    max as f64,
                    &logger,
                );
            }

            logger.debug(&format!("Processed timing {name}"));
        });
    }
}

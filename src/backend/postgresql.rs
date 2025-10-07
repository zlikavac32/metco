use crate::backend::{Backend, Logger};
use crate::metrics::TimeFrame;
use chrono::{DateTime, Utc};
use postgres_types::Json;
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
/*
Used table structure is bellow.

create extension btree_gin;

create table metric_counters
(
    name  text        not null,
    time  timestamptz not null,
    value float8      not null,
    tags  jsonb       not null
);

create index index_metric_counters on metric_counters (name, time desc);
create index index_metric_counters_tags on metric_counters using gin (name, tags, time);

create table metric_histograms
(
    name  text        not null,
    time  timestamptz not null,
    value float8      not null,
    tags  jsonb       not null
);

create index index_metric_histograms on metric_histograms (name, time desc);
create index index_metric_histograms_tags on metric_histograms using gin (name, tags, time);

create table metric_timers
(
    name  text        not null,
    time  timestamptz not null,
    value float8      not null,
    tags  jsonb       not null
);

create index index_metric_timers on metric_timers (name, time desc);
create index index_metric_timers_tags on metric_timers using gin (name, tags, time);

create table metric_gauges
(
    name  text        not null,
    time  timestamptz not null,
    value float8      not null,
    tags  jsonb       not null
);

create index index_metric_gauges on metric_gauges (name, time desc);
create index index_metric_gauges_tags on metric_gauges using gin (name, tags, time);

It's also possible to use https://github.com/timescale/timescaledb and
slightly modify create commands above to use hyper-tables.

create extension btree_gin;

create table metric_counters
(
    name  text        not null,
    time  timestamptz not null,
    value float8      not null,
    tags  jsonb       not null
)
    with (
        timescaledb.hypertable,
        timescaledb.partition_column = 'time',
        timescaledb.segmentby = 'name',
        timescaledb.create_default_indexes = false
        );

create index index_metric_counters on metric_counters (name, time desc);
create index index_metric_counters_tags on metric_counters using gin (name, tags, time);

create table metric_histograms
(
    name  text        not null,
    time  timestamptz not null,
    value float8      not null,
    tags  jsonb       not null
)
    with (
        timescaledb.hypertable,
        timescaledb.partition_column = 'time',
        timescaledb.segmentby = 'name',
        timescaledb.create_default_indexes = false
        );

create index index_metric_histograms on metric_histograms (name, time desc);
create index index_metric_histograms_tags on metric_histograms using gin (name, tags, time);

create table metric_timers
(
    name  text        not null,
    time  timestamptz not null,
    value float8      not null,
    tags  jsonb       not null
)
    with (
        timescaledb.hypertable,
        timescaledb.partition_column = 'time',
        timescaledb.segmentby = 'name',
        timescaledb.create_default_indexes = false
        );

create index index_metric_timers on metric_timers (name, time desc);
create index index_metric_timers_tags on metric_timers using gin (name, tags, time);

create table metric_gauges
(
    name  text        not null,
    time  timestamptz not null,
    value float8      not null,
    tags  jsonb       not null
)
    with (
        timescaledb.hypertable,
        timescaledb.partition_column = 'time',
        timescaledb.segmentby = 'name',
        timescaledb.create_default_indexes = false
        );

create index index_metric_gauges on metric_gauges (name, time desc);
create index index_metric_gauges_tags on metric_gauges using gin (name, tags, time);
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
    Histogram,
    Timer,
}

impl PostgreSQL {
    pub fn new(client: postgres::Client) -> Self {
        Self { client }
    }

    fn insert(
        &mut self,
        time: &DateTime<Utc>,
        metric_kind: MetricKind,
        name: &str,
        tags: &HashMap<String, String>,
        value: f64,
        logger: &Logger,
    ) {
        let table_name = match metric_kind {
            MetricKind::Gauge => "metric_gauges",
            MetricKind::Counter => "metric_counters",
            MetricKind::Histogram => "metric_Histograms",
            MetricKind::Timer => "metric_timers",
        };

        let sql = format!(
            r"
insert into {table_name} (name, time, value, tags)
values ($1, $2, $3, $4)
"
        );

        if let Err(err) = self
            .client
            .execute(&sql, &[&name, time, &value, &Json::<_>(tags)])
        {
            logger.error(&format!("Insert record failed: {err}"));
        }
    }
}

impl Backend for PostgreSQL {
    fn publish(&mut self, time: &DateTime<Utc>, time_frame: &TimeFrame, logger: Logger) {
        time_frame
            .gauges()
            .iter()
            .for_each(|(identifier, (current, stats))| {
                let name = identifier.name();
                let tags = identifier.tags();

                self.insert(
                    time,
                    MetricKind::Gauge,
                    &format!("{name}.current"),
                    tags,
                    (*current) as f64,
                    &logger,
                );
                self.insert(
                    time,
                    MetricKind::Gauge,
                    &format!("{name}.count"),
                    tags,
                    stats.count() as f64,
                    &logger,
                );
                self.insert(
                    time,
                    MetricKind::Gauge,
                    &format!("{name}.sum"),
                    tags,
                    stats.sum() as f64,
                    &logger,
                );
                self.insert(
                    time,
                    MetricKind::Gauge,
                    &format!("{name}.std"),
                    tags,
                    stats.std(),
                    &logger,
                );
                self.insert(
                    time,
                    MetricKind::Gauge,
                    &format!("{name}.median"),
                    tags,
                    stats.median() as f64,
                    &logger,
                );
                self.insert(
                    time,
                    MetricKind::Gauge,
                    &format!("{name}.p75"),
                    tags,
                    stats.percentile(75.into()) as f64,
                    &logger,
                );
                self.insert(
                    time,
                    MetricKind::Gauge,
                    &format!("{name}.p90"),
                    tags,
                    stats.percentile(90.into()) as f64,
                    &logger,
                );
                self.insert(
                    time,
                    MetricKind::Gauge,
                    &format!("{name}.p99"),
                    tags,
                    stats.percentile(99.into()) as f64,
                    &logger,
                );
                self.insert(
                    time,
                    MetricKind::Gauge,
                    &format!("{name}.min"),
                    tags,
                    stats.min() as f64,
                    &logger,
                );
                self.insert(
                    time,
                    MetricKind::Gauge,
                    &format!("{name}.max"),
                    tags,
                    stats.max() as f64,
                    &logger,
                );

                logger.debug(&format!("Processed gauge {name} with tags {tags:?}"));
            });

        time_frame
            .counters()
            .iter()
            .for_each(|(identifier, value)| {
                self.insert(
                    time,
                    MetricKind::Counter,
                    identifier.name(),
                    identifier.tags(),
                    *value as f64,
                    &logger,
                );

                logger.debug(&format!(
                    "Processed counter {} with tags {:?}",
                    identifier.name(),
                    identifier.tags()
                ));
            });

        time_frame
            .histograms()
            .iter()
            .for_each(|(identifier, stats)| {
                let name = identifier.name();
                let tags = identifier.tags();

                self.insert(
                    time,
                    MetricKind::Histogram,
                    &format!("{name}.count"),
                    tags,
                    stats.count() as f64,
                    &logger,
                );
                self.insert(
                    time,
                    MetricKind::Histogram,
                    &format!("{name}.sum"),
                    tags,
                    stats.sum() as f64,
                    &logger,
                );
                self.insert(
                    time,
                    MetricKind::Histogram,
                    &format!("{name}.std"),
                    tags,
                    stats.std(),
                    &logger,
                );
                self.insert(
                    time,
                    MetricKind::Histogram,
                    &format!("{name}.median"),
                    tags,
                    stats.median() as f64,
                    &logger,
                );
                self.insert(
                    time,
                    MetricKind::Histogram,
                    &format!("{name}.p75"),
                    tags,
                    stats.percentile(75.into()) as f64,
                    &logger,
                );
                self.insert(
                    time,
                    MetricKind::Histogram,
                    &format!("{name}.p90"),
                    tags,
                    stats.percentile(90.into()) as f64,
                    &logger,
                );
                self.insert(
                    time,
                    MetricKind::Histogram,
                    &format!("{name}.p99"),
                    tags,
                    stats.percentile(99.into()) as f64,
                    &logger,
                );
                self.insert(
                    time,
                    MetricKind::Histogram,
                    &format!("{name}.min"),
                    tags,
                    stats.min() as f64,
                    &logger,
                );
                self.insert(
                    time,
                    MetricKind::Histogram,
                    &format!("{name}.max"),
                    tags,
                    stats.max() as f64,
                    &logger,
                );

                logger.debug(&format!("Processed histogram {name} with tags {tags:?}"));
            });

        time_frame.timings().iter().for_each(|(identifier, stats)| {
            let name = identifier.name();
            let tags = identifier.tags();

            self.insert(
                time,
                MetricKind::Timer,
                &format!("{name}.count"),
                tags,
                stats.count() as f64,
                &logger,
            );
            self.insert(
                time,
                MetricKind::Timer,
                &format!("{name}.sum"),
                tags,
                stats.sum() as f64,
                &logger,
            );
            self.insert(
                time,
                MetricKind::Timer,
                &format!("{name}.std"),
                tags,
                stats.std(),
                &logger,
            );
            self.insert(
                time,
                MetricKind::Timer,
                &format!("{name}.median"),
                tags,
                stats.median() as f64,
                &logger,
            );
            self.insert(
                time,
                MetricKind::Timer,
                &format!("{name}.p75"),
                tags,
                stats.percentile(75.into()) as f64,
                &logger,
            );
            self.insert(
                time,
                MetricKind::Timer,
                &format!("{name}.p90"),
                tags,
                stats.percentile(90.into()) as f64,
                &logger,
            );
            self.insert(
                time,
                MetricKind::Timer,
                &format!("{name}.p99"),
                tags,
                stats.percentile(99.into()) as f64,
                &logger,
            );
            self.insert(
                time,
                MetricKind::Timer,
                &format!("{name}.min"),
                tags,
                stats.min() as f64,
                &logger,
            );
            self.insert(
                time,
                MetricKind::Timer,
                &format!("{name}.max"),
                tags,
                stats.max() as f64,
                &logger,
            );

            logger.debug(&format!("Processed timing {name} with tags {tags:?}"));
        });
    }
}

use crate::backend::{Backend, Logger};
use crate::metrics::{Identifier, Statistics, TimeFrame};
use chrono::{DateTime, Utc};
use postgres_types::Json;
use std::collections::HashMap;
use std::fmt::{self, Debug, Formatter};
/*
Used table structure is below.

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

// Manual implementation because `postgres::Client` does not implement `Debug`.
impl Debug for PostgreSQL {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("PostgreSQL").finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy)]
enum Table {
    Gauges,
    Counters,
    Histograms,
    Timers,
}

impl Table {
    fn insert_sql(self) -> &'static str {
        match self {
            Table::Gauges => {
                "insert into metric_gauges (name, time, value, tags) values ($1, $2, $3, $4)"
            }
            Table::Counters => {
                "insert into metric_counters (name, time, value, tags) values ($1, $2, $3, $4)"
            }
            Table::Histograms => {
                "insert into metric_histograms (name, time, value, tags) values ($1, $2, $3, $4)"
            }
            Table::Timers => {
                "insert into metric_timers (name, time, value, tags) values ($1, $2, $3, $4)"
            }
        }
    }
}

impl PostgreSQL {
    pub fn new(client: postgres::Client) -> Self {
        Self { client }
    }

    fn insert(
        &mut self,
        time: &DateTime<Utc>,
        table: Table,
        name: &str,
        tags: &HashMap<String, String>,
        value: f64,
        logger: &Logger,
    ) {
        if let Err(err) = self
            .client
            .execute(table.insert_sql(), &[&name, time, &value, &Json(tags)])
        {
            logger.error(format_args!("Insert record failed: {err}"));
        }
    }

    fn insert_statistics(
        &mut self,
        time: &DateTime<Utc>,
        table: Table,
        identifier: &Identifier,
        statistics: &Statistics,
        logger: &Logger,
    ) {
        for (stat, value) in statistics.summary() {
            self.insert(
                time,
                table,
                &format!("{}.{stat}", identifier.name()),
                identifier.tags(),
                value.into(),
                logger,
            );
        }
    }
}

impl Backend for PostgreSQL {
    fn publish(&mut self, time: &DateTime<Utc>, time_frame: &TimeFrame, logger: &Logger) {
        for (identifier, gauge) in time_frame.gauges() {
            let name = identifier.name();
            let tags = identifier.tags();

            self.insert(
                time,
                Table::Gauges,
                &format!("{name}.current"),
                tags,
                gauge.current as f64,
                logger,
            );
            self.insert_statistics(time, Table::Gauges, identifier, &gauge.statistics, logger);

            logger.debug(format_args!("Processed gauge {name} with tags {tags:?}"));
        }

        for (identifier, value) in time_frame.counters() {
            let name = identifier.name();
            let tags = identifier.tags();

            self.insert(time, Table::Counters, name, tags, *value as f64, logger);

            logger.debug(format_args!("Processed counter {name} with tags {tags:?}"));
        }

        for (identifier, statistics) in time_frame.histograms() {
            self.insert_statistics(time, Table::Histograms, identifier, statistics, logger);

            logger.debug(format_args!(
                "Processed histogram {} with tags {:?}",
                identifier.name(),
                identifier.tags()
            ));
        }

        for (identifier, statistics) in time_frame.timings() {
            self.insert_statistics(time, Table::Timers, identifier, statistics, logger);

            logger.debug(format_args!(
                "Processed timing {} with tags {:?}",
                identifier.name(),
                identifier.tags()
            ));
        }
    }
}

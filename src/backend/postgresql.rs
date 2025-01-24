use crate::backend::{Backend, Logger};
use crate::metrics::TimeFrame;
use chrono::{DateTime, Utc};
use postgres::types::ToSql;
use std::fmt::{Debug, Formatter};

/*
create type metric_kind as enum ('gauge', 'counter', 'timing');

create table metrics
(
    name  text        not null,
    kind  metric_kind not null,
    time  timestamptz not null,
    host  text        not null,
    value float8,
    primary key (name, kind, time, host)
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

    fn insert(
        &mut self,
        time: &DateTime<Utc>,
        host: &str,
        metric_kind: MetricKind,
        name: &str,
        value: f64,
    ) {
        let sql = r"
insert into metrics (name, kind, time, host, value)
values ($1, $2, $3, $4, $5)
on conflict (name, kind, time, host)
    do nothing
";

        if let Err(err) = self
            .client
            .execute(sql, &[&name, &metric_kind, time, &host, &value])
        {
            log::error!("Postgresql failed to insert record: {err}");
        }
    }
}

impl Backend for PostgreSQL {
    fn publish(&mut self, time: &DateTime<Utc>, time_frame: &TimeFrame, logger: Logger) {
        time_frame.gauges.iter().for_each(|(name, value)| {
            self.insert(
                time,
                &time_frame.host,
                MetricKind::Gauge,
                name,
                *value as f64,
            );

            logger.debug(&format!("Inserted gauge {name}"));
        });

        time_frame.counters.iter().for_each(|(name, stats)| {
            self.insert(
                time,
                &time_frame.host,
                MetricKind::Counter,
                &format!("{name}.count"),
                stats.count() as f64,
            );
            self.insert(
                time,
                &time_frame.host,
                MetricKind::Counter,
                &format!("{name}.sum"),
                stats.sum() as f64,
            );
            self.insert(
                time,
                &time_frame.host,
                MetricKind::Counter,
                &format!("{name}.avg"),
                stats.average(),
            );
            self.insert(
                time,
                &time_frame.host,
                MetricKind::Counter,
                &format!("{name}.std"),
                stats.std(),
            );
            self.insert(
                time,
                &time_frame.host,
                MetricKind::Counter,
                &format!("{name}.median"),
                stats.median(),
            );
            self.insert(
                time,
                &time_frame.host,
                MetricKind::Counter,
                &format!("{name}.p75"),
                stats.percentile(0.75) as f64,
            );
            self.insert(
                time,
                &time_frame.host,
                MetricKind::Counter,
                &format!("{name}.p90"),
                stats.percentile(0.90) as f64,
            );

            logger.debug(&format!("Inserted counter {name}"));
        });

        time_frame.timings.iter().for_each(|(name, stats)| {
            self.insert(
                time,
                &time_frame.host,
                MetricKind::Timing,
                &format!("{name}.count"),
                stats.count() as f64,
            );
            self.insert(
                time,
                &time_frame.host,
                MetricKind::Timing,
                &format!("{name}.sum"),
                stats.sum() as f64,
            );
            self.insert(
                time,
                &time_frame.host,
                MetricKind::Timing,
                &format!("{name}.avg"),
                stats.average(),
            );
            self.insert(
                time,
                &time_frame.host,
                MetricKind::Timing,
                &format!("{name}.std"),
                stats.std(),
            );
            self.insert(
                time,
                &time_frame.host,
                MetricKind::Timing,
                &format!("{name}.median"),
                stats.median(),
            );
            self.insert(
                time,
                &time_frame.host,
                MetricKind::Timing,
                &format!("{name}.p75"),
                stats.percentile(0.75) as f64,
            );
            self.insert(
                time,
                &time_frame.host,
                MetricKind::Timing,
                &format!("{name}.p90"),
                stats.percentile(0.90) as f64,
            );

            logger.debug(&format!("Inserted timing {name}"));
        });
    }
}

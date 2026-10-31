use crate::backend::{Backend, Logger};
use crate::metrics::{Identifier, Number, Statistics, TimeFrame};
use chrono::{DateTime, Utc};
use reqwest::blocking::Client;
use serde_json::{Map, Value};
use std::fmt::{self, Debug, Formatter};

pub struct ElasticSearch {
    client: Client,
    dsn: String,
    index_format: String,
}

// Manual implementation so that credentials in `dsn` don't end up in logs.
impl Debug for ElasticSearch {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("ElasticSearch").finish_non_exhaustive()
    }
}

impl From<Number> for Value {
    fn from(number: Number) -> Self {
        match number {
            Number::Integer(value) => value.into(),
            Number::Float(value) => value.into(),
        }
    }
}

fn statistics_fields(prefix: &str, statistics: &Statistics) -> Map<String, Value> {
    statistics
        .summary()
        .into_iter()
        .map(|(name, value)| (format!("{prefix}.{name}"), value.into()))
        .collect()
}

impl ElasticSearch {
    pub fn new(client: Client, dsn: String, index_format: String) -> Self {
        Self {
            client,
            dsn,
            index_format,
        }
    }

    fn insert(
        &self,
        url: &str,
        time: &DateTime<Utc>,
        mut document: Map<String, Value>,
        identifier: &Identifier,
        logger: &Logger,
    ) {
        document.insert("timestamp".into(), time.to_rfc3339().into());
        document.insert(
            "tags".into(),
            identifier
                .tags()
                .iter()
                .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                .collect::<Map<_, _>>()
                .into(),
        );

        let result = self
            .client
            .post(url)
            .json(&document)
            .send()
            .and_then(|response| response.error_for_status());

        if let Err(err) = result {
            logger.error(format_args!("Publish to index failed: {err}"));
        }
    }
}

impl Backend for ElasticSearch {
    fn publish(&mut self, time: &DateTime<Utc>, time_frame: &TimeFrame, logger: &Logger) {
        let index = self
            .index_format
            .replace("{date}", &time.format("%Y-%m-%d").to_string());
        let url = format!("{}/{index}/_doc", self.dsn);

        for (identifier, gauge) in time_frame.gauges() {
            let name = identifier.name();

            let mut document = Map::new();
            document.insert(format!("gauge.{name}.current"), gauge.current.into());
            document.extend(statistics_fields(
                &format!("gauge.{name}"),
                &gauge.statistics,
            ));

            self.insert(&url, time, document, identifier, logger);

            logger.debug(format_args!(
                "Processed gauge {name} with tags {:?}",
                identifier.tags()
            ));
        }

        for (identifier, value) in time_frame.counters() {
            let name = identifier.name();

            let document = Map::from_iter([(format!("counter.{name}"), (*value).into())]);

            self.insert(&url, time, document, identifier, logger);

            logger.debug(format_args!(
                "Processed counter {name} with tags {:?}",
                identifier.tags()
            ));
        }

        for (identifier, statistics) in time_frame.histograms() {
            let name = identifier.name();

            let document = statistics_fields(&format!("histogram.{name}"), statistics);

            self.insert(&url, time, document, identifier, logger);

            logger.debug(format_args!(
                "Processed histogram {name} with tags {:?}",
                identifier.tags()
            ));
        }

        for (identifier, statistics) in time_frame.timings() {
            let name = identifier.name();

            let document = statistics_fields(&format!("timing.{name}"), statistics);

            self.insert(&url, time, document, identifier, logger);

            logger.debug(format_args!(
                "Processed timing {name} with tags {:?}",
                identifier.tags()
            ));
        }
    }
}

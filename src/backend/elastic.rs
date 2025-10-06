use crate::backend::{Backend, Logger};
use crate::metrics::TimeFrame;
use chrono::{DateTime, Utc};
use reqwest::blocking::Client;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};

pub struct ElasticSearch {
    client: Client,
    dsn: String,
    index_format: String,
}

impl Debug for ElasticSearch {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("ElasticSearch {}")
    }
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
        &mut self,
        time: &DateTime<Utc>,
        mut map: HashMap<String, Value>,
        tags: &HashMap<String, String>,
        logger: &Logger,
    ) {
        let index = self
            .index_format
            .clone()
            .replace("{date}", &time.format("%Y-%m-%d").to_string());

        map.insert("timestamp".into(), time.to_rfc3339().into());
        map.insert(
            "tags".into(),
            tags.iter()
                .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                .collect::<Map<String, Value>>()
                .into(),
        );

        let err = match self
            .client
            .post(format!("{dsn}/{index}/_doc", dsn = self.dsn))
            .json(&map)
            .send()
        {
            Ok(response) => match response.error_for_status() {
                Ok(_) => return,
                Err(err) => err,
            },
            Err(err) => err,
        };

        logger.error(&format!("Publish to index failed: {err}"));
    }
}

impl Backend for ElasticSearch {
    fn publish(&mut self, time: &DateTime<Utc>, time_frame: &TimeFrame, logger: Logger) {
        time_frame.gauges().iter().for_each(|(identifier, value)| {
            self.insert(
                time,
                HashMap::from([(format!("gauge.{}", identifier.name()), (*value).into())]),
                identifier.tags(),
                &logger,
            );

            logger.debug(&format!(
                "Processed gauge {} with tags {:?}",
                identifier.name(),
                identifier.tags()
            ));
        });

        time_frame
            .counters()
            .iter()
            .for_each(|(identifier, value)| {
                self.insert(
                    time,
                    HashMap::from([(format!("counter.{}", identifier.name()), (*value).into())]),
                    identifier.tags(),
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

                let map = HashMap::from([
                    (format!("histogram.{name}.count"), stats.count().into()),
                    (format!("histogram.{name}.sum"), stats.sum().into()),
                    (format!("histogram.{name}.std"), stats.std().into()),
                    (format!("histogram.{name}.median"), stats.median().into()),
                    (
                        format!("histogram.{name}.p75"),
                        stats.percentile(75.into()).into(),
                    ),
                    (
                        format!("histogram.{name}.p90"),
                        stats.percentile(90.into()).into(),
                    ),
                    (
                        format!("histogram.{name}.p99"),
                        stats.percentile(99.into()).into(),
                    ),
                    (format!("histogram.{name}.min"), stats.min().into()),
                    (format!("histogram.{name}.max"), stats.max().into()),
                ]);

                self.insert(time, map, identifier.tags(), &logger);

                logger.debug(&format!(
                    "Processed histogram {name} with tags {:?}",
                    identifier.tags()
                ));
            });

        time_frame.timings().iter().for_each(|(identifier, stats)| {
            let name = identifier.name();

            let map = HashMap::from([
                (format!("timing.{name}.count"), stats.count().into()),
                (format!("timing.{name}.sum"), stats.sum().into()),
                (format!("timing.{name}.std"), stats.std().into()),
                (format!("timing.{name}.median"), stats.median().into()),
                (
                    format!("timing.{name}.p75"),
                    stats.percentile(75.into()).into(),
                ),
                (
                    format!("timing.{name}.p90"),
                    stats.percentile(90.into()).into(),
                ),
                (
                    format!("timing.{name}.p99"),
                    stats.percentile(99.into()).into(),
                ),
                (format!("counter.{name}.min"), stats.min().into()),
                (format!("counter.{name}.max"), stats.max().into()),
            ]);

            self.insert(time, map, identifier.tags(), &logger);

            logger.debug(&format!(
                "Processed timing {name} with tags {:?}",
                identifier.tags()
            ));
        });
    }
}

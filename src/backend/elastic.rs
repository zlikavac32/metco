use crate::backend::{Backend, Logger};
use crate::metrics::TimeFrame;
use chrono::{DateTime, Utc};
use reqwest::blocking::Client;
use serde_json::Value;
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
        host: &str,
        mut map: HashMap<String, Value>,
        logger: &Logger,
    ) {
        let index = self
            .index_format
            .clone()
            .replace("{date}", &time.format("%Y-%m-%d").to_string());

        map.insert("host".into(), host.into());
        map.insert("timestamp".into(), time.to_rfc3339().into());

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
        time_frame.gauges.iter().for_each(|(name, value)| {
            self.insert(
                time,
                &time_frame.host,
                HashMap::from([(format!("gauge.{name}"), (*value).into())]),
                &logger,
            );

            logger.debug(&format!("Processed gauge {name}"));
        });

        time_frame.counters.iter().for_each(|(name, stats)| {
            self.insert(
                time,
                &time_frame.host,
                HashMap::from([
                    (format!("counter.{name}.count"), stats.count().into()),
                    (format!("counter.{name}.sum"), stats.sum().into()),
                    (format!("counter.{name}.std"), stats.std().into()),
                    (format!("counter.{name}.median"), stats.median().into()),
                    (format!("counter.{name}.p75"), stats.percentile(0.75).into()),
                    (format!("counter.{name}.p90"), stats.percentile(0.90).into()),
                ]),
                &logger,
            );

            logger.debug(&format!("Processed counter {name}"));
        });

        time_frame.timings.iter().for_each(|(name, stats)| {
            self.insert(
                time,
                &time_frame.host,
                HashMap::from([
                    (format!("timing.{name}.count"), stats.count().into()),
                    (format!("timing.{name}.sum"), stats.sum().into()),
                    (format!("timing.{name}.std"), stats.std().into()),
                    (format!("timing.{name}.median"), stats.median().into()),
                    (format!("timing.{name}.p75"), stats.percentile(0.75).into()),
                    (format!("timing.{name}.p90"), stats.percentile(0.90).into()),
                ]),
                &logger,
            );

            logger.debug(&format!("Processed timing {name}"));
        });
    }
}

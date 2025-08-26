use std::collections::HashMap;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, PartialEq)]
pub enum TimerResolution {
    Seconds,
    MilliSeconds,
    MicroSeconds,
    NanoSeconds,
}

#[derive(Debug, PartialEq)]
pub enum GaugeOperation {
    Set(i64),
    Modify(i64),
    Remove,
}

#[derive(Debug, PartialEq)]
pub enum MetricKind {
    Counter(u64),
    Timing(u64, TimerResolution),
    Gauge(GaugeOperation),
}

#[derive(Debug, PartialEq)]
pub struct Identifier {
    name: String,
}

#[derive(Debug, PartialEq)]
pub struct Metric {
    identifier: Identifier,
    kind: MetricKind,
}

impl Identifier {
    pub fn without_tags(name: String) -> Self {
        Self { name }
    }
}

impl Metric {
    pub fn new(identifier: Identifier, kind: MetricKind) -> Self {
        Self { identifier, kind }
    }

    pub fn is_counter(&self) -> bool {
        matches!(self.kind, MetricKind::Counter(_))
    }

    pub fn is_timer(&self) -> bool {
        matches!(self.kind, MetricKind::Timing(_, _))
    }

    pub fn is_gauge(&self) -> bool {
        matches!(self.kind, MetricKind::Gauge(_))
    }
}

#[derive(Debug)]
pub struct Statistics {
    list: Vec<u64>,
    sum: u64,
    std: f64,
}

impl Statistics {
    fn new(mut list: Vec<u64>) -> Result<Self, ()> {
        assert!(!list.is_empty());

        list.sort();

        let mut sum = 0u64;

        for item in &list {
            match sum.checked_add(*item) {
                Some(val) => sum = val,
                None => return Err(()),
            }
        }

        let avg = sum as f64 / list.len() as f64;
        let std = list
            .iter()
            .fold(0., |acc, item| acc + (*item as f64 - avg).powf(2.))
            .powf(0.5);

        Ok(Self { list, sum, std })
    }

    pub fn sum(&self) -> u64 {
        self.sum
    }

    pub fn count(&self) -> usize {
        self.list.len()
    }

    pub fn min(&self) -> Option<u64> {
        self.list.first().copied()
    }

    pub fn max(&self) -> Option<u64> {
        self.list.last().copied()
    }

    pub fn median(&self) -> f64 {
        let len = self.list.len();

        if len & 1 == 0 {
            (self.list[len / 2 - 1] as f64 + self.list[len / 2] as f64) / 2.
        } else {
            self.list[len / 2] as f64
        }
    }

    pub fn std(&self) -> f64 {
        self.std
    }

    pub fn percentile(&self, p: f64) -> u64 {
        self.list
            [((self.list.len() as f64 * p.clamp(0., 1.)).floor() as usize).min(self.list.len())]
    }
}

#[derive(Debug)]
pub struct TimeFrame {
    counters: HashMap<String, Statistics>,
    gauges: HashMap<String, i64>,
    timings: HashMap<String, Statistics>,
    host: String,
}

impl TimeFrame {
    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn counters(&self) -> &HashMap<String, Statistics> {
        &self.counters
    }

    pub fn gauges(&self) -> &HashMap<String, i64> {
        &self.gauges
    }

    pub fn timings(&self) -> &HashMap<String, Statistics> {
        &self.timings
    }
}

#[derive(Debug)]
pub enum OverflowingMetric {
    Counter(String),
    Timing(String),
}

impl Display for OverflowingMetric {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let (kind, name) = match self {
            OverflowingMetric::Counter(name) => ("counter", name),
            OverflowingMetric::Timing(name) => ("timing", name),
        };

        f.write_str(kind)?;
        f.write_str(": ")?;
        f.write_str(name)
    }
}

#[derive(Debug, Default)]
pub struct Registry {
    counters: HashMap<String, Vec<u64>>,
    gauges: HashMap<String, i64>,
    timings: HashMap<String, Vec<u64>>,
}

impl Registry {
    pub fn is_empty(&self) -> bool {
        self.counters.is_empty() && self.gauges.is_empty() && self.timings.is_empty()
    }
}

impl Registry {
    pub fn add(&mut self, metric: Metric) -> bool {
        match metric.kind {
            MetricKind::Counter(value) => self
                .counters
                .entry(metric.identifier.name)
                .or_default()
                .push(value),
            MetricKind::Timing(value, resolution) => self
                .timings
                .entry(metric.identifier.name)
                .or_default()
                .push(
                    match value.checked_mul(match resolution {
                        TimerResolution::Seconds => 1_000_000_000,
                        TimerResolution::MilliSeconds => 1_000_000,
                        TimerResolution::MicroSeconds => 1_000,
                        TimerResolution::NanoSeconds => 1,
                    }) {
                        None => return false,
                        Some(res) => res,
                    },
                ),
            MetricKind::Gauge(operation) => match operation {
                GaugeOperation::Set(value) => {
                    self.gauges.insert(metric.identifier.name, value);
                }
                GaugeOperation::Modify(value) => {
                    let val = self.gauges.entry(metric.identifier.name).or_default();

                    match val.checked_add(value) {
                        None => return false,
                        Some(res) => *val = res,
                    }
                }
                GaugeOperation::Remove => {
                    self.gauges.remove(&metric.identifier.name);
                }
            },
        }

        true
    }

    pub fn new_with_gauges(&self) -> Self {
        Self {
            gauges: self.gauges.clone(),
            ..Default::default()
        }
    }

    pub fn finalize(self) -> Option<(TimeFrame, Vec<OverflowingMetric>)> {
        let host = hostname::get().ok()?.into_string().ok()?;

        let mut overflowing_metrics = vec![];

        let time_frame = TimeFrame {
            gauges: self.gauges,
            counters: self.counters.into_iter().fold(
                HashMap::default(),
                |mut map, (name, list)| {
                    if let Ok(statistics) = Statistics::new(list) {
                        map.insert(name, statistics);
                    } else {
                        overflowing_metrics.push(OverflowingMetric::Counter(name));
                    }

                    map
                },
            ),
            timings: self
                .timings
                .into_iter()
                .fold(HashMap::default(), |mut map, (name, list)| {
                    if let Ok(statistics) = Statistics::new(list) {
                        map.insert(name, statistics);
                    } else {
                        overflowing_metrics.push(OverflowingMetric::Timing(name));
                    }

                    map
                }),
            host,
        };

        Some((time_frame, overflowing_metrics))
    }
}

#[cfg(test)]
mod test {

    use super::*;

    #[test]
    fn counter_can_be_added() {
        let mut registry = Registry::default();

        let mut map = HashMap::default();
        map.insert("test".into(), vec![2, 7]);
        map.insert("demo".into(), vec![32]);

        assert!(registry.add(Metric::new(
            Identifier::without_tags("test".into()),
            MetricKind::Counter(2)
        )));
        assert!(registry.add(Metric::new(
            Identifier::without_tags("demo".into()),
            MetricKind::Counter(32)
        )));
        assert!(registry.add(Metric::new(
            Identifier::without_tags("test".into()),
            MetricKind::Counter(7)
        )));

        assert_eq!(map, registry.counters)
    }

    #[test]
    fn timings_can_be_added() {
        let mut registry = Registry::default();

        let mut map = HashMap::default();
        map.insert("test".into(), vec![2, 7_000]);
        map.insert("demo".into(), vec![32_000_000, 64_000_000_000]);

        assert!(registry.add(Metric::new(
            Identifier::without_tags("test".into()),
            MetricKind::Timing(2, TimerResolution::NanoSeconds)
        )));
        assert!(registry.add(Metric::new(
            Identifier::without_tags("demo".into()),
            MetricKind::Timing(32, TimerResolution::MilliSeconds)
        )));
        assert!(registry.add(Metric::new(
            Identifier::without_tags("test".into()),
            MetricKind::Timing(7, TimerResolution::MicroSeconds)
        )));
        assert!(registry.add(Metric::new(
            Identifier::without_tags("demo".into()),
            MetricKind::Timing(64, TimerResolution::Seconds)
        )));

        assert_eq!(map, registry.timings)
    }

    #[test]
    fn gauges_can_be_added() {
        let mut registry = Registry::default();

        let mut map = HashMap::default();
        map.insert("test".into(), 10);

        assert!(registry.add(Metric::new(
            Identifier::without_tags("test".into()),
            MetricKind::Gauge(GaugeOperation::Modify(10))
        )));

        assert_eq!(map, registry.gauges);

        let mut map = HashMap::default();
        map.insert("test".into(), -10);

        assert!(registry.add(Metric::new(
            Identifier::without_tags("test".into()),
            MetricKind::Gauge(GaugeOperation::Modify(-20))
        )));

        assert_eq!(map, registry.gauges);

        let mut map = HashMap::default();
        map.insert("test".into(), 32);

        assert!(registry.add(Metric::new(
            Identifier::without_tags("test".into()),
            MetricKind::Gauge(GaugeOperation::Set(32))
        )));

        assert_eq!(map, registry.gauges);

        assert!(registry.add(Metric::new(
            Identifier::without_tags("test".into()),
            MetricKind::Gauge(GaugeOperation::Remove)
        )));

        assert_eq!(HashMap::default(), registry.gauges);
    }
}

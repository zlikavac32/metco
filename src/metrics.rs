use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::hash::{Hash, Hasher};

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

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Identifier {
    name: String,
    tags: HashMap<String, String>,
}

impl Identifier {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn tags(&self) -> &HashMap<String, String> {
        &self.tags
    }
}

impl Hash for Identifier {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write(self.name.as_bytes());

        for (k, v) in self.tags.iter() {
            state.write(k.as_bytes());
            state.write(v.as_bytes());
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Metric {
    identifier: Identifier,
    kind: MetricKind,
}

impl Identifier {
    pub fn without_tags(name: String) -> Self {
        Self {
            name,
            tags: HashMap::default(),
        }
    }
    pub fn with_tags(name: String, tags: HashMap<String, String>) -> Self {
        Self { name, tags }
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

pub struct Percentile(f64);

impl From<u32> for Percentile {
    fn from(value: u32) -> Self {
        if value > 100 {
            panic!("Value {} out of range [0, 100]", value);
        }

        Self(value as f64)
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

    pub fn median(&self) -> u64 {
        self.percentile(50.into())
    }

    pub fn std(&self) -> f64 {
        self.std
    }

    pub fn percentile(&self, p: Percentile) -> u64 {
        self.list[(self.list.len() as f64 * (p.0 / 100. - 0.01)).floor() as usize]
    }
}

#[derive(Debug)]
pub struct TimeFrame {
    counters: HashMap<Identifier, Statistics>,
    gauges: HashMap<Identifier, i64>,
    timings: HashMap<Identifier, Statistics>,
}

impl TimeFrame {
    pub fn counters(&self) -> &HashMap<Identifier, Statistics> {
        &self.counters
    }

    pub fn gauges(&self) -> &HashMap<Identifier, i64> {
        &self.gauges
    }

    pub fn timings(&self) -> &HashMap<Identifier, Statistics> {
        &self.timings
    }
}

#[derive(Debug)]
pub enum OverflowingMetric {
    Counter(Identifier),
    Timing(Identifier),
}

impl Display for OverflowingMetric {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let (kind, identifier) = match self {
            OverflowingMetric::Counter(name) => ("counter", name),
            OverflowingMetric::Timing(name) => ("timing", name),
        };

        f.write_str(kind)?;
        f.write_str(": ")?;
        f.write_str(&identifier.name)?;

        if !identifier.tags.is_empty() {
            todo!("Tags!");
        }

        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct Registry {
    counters: HashMap<Identifier, Vec<u64>>,
    gauges: HashMap<Identifier, i64>,
    timings: HashMap<Identifier, Vec<u64>>,
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
                .entry(metric.identifier)
                .or_default()
                .push(value),
            MetricKind::Timing(value, resolution) => {
                self.timings.entry(metric.identifier).or_default().push(
                    match value.checked_mul(match resolution {
                        TimerResolution::Seconds => 1_000_000_000,
                        TimerResolution::MilliSeconds => 1_000_000,
                        TimerResolution::MicroSeconds => 1_000,
                        TimerResolution::NanoSeconds => 1,
                    }) {
                        None => return false,
                        Some(res) => res,
                    },
                )
            }
            MetricKind::Gauge(operation) => match operation {
                GaugeOperation::Set(value) => {
                    self.gauges.insert(metric.identifier, value);
                }
                GaugeOperation::Modify(value) => {
                    let val = self.gauges.entry(metric.identifier).or_default();

                    match val.checked_add(value) {
                        None => return false,
                        Some(res) => *val = res,
                    }
                }
                GaugeOperation::Remove => {
                    self.gauges.remove(&metric.identifier);
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
        let mut overflowing_metrics = vec![];

        let time_frame = TimeFrame {
            gauges: self.gauges,
            counters: self.counters.into_iter().fold(
                HashMap::default(),
                |mut map, (identifier, list)| {
                    if let Ok(statistics) = Statistics::new(list) {
                        map.insert(identifier, statistics);
                    } else {
                        overflowing_metrics.push(OverflowingMetric::Counter(identifier));
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
        map.insert(Identifier::without_tags("test".into()), vec![2, 7]);
        map.insert(Identifier::without_tags("demo".into()), vec![32]);

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
        map.insert(Identifier::without_tags("test".into()), vec![2, 7_000]);
        map.insert(
            Identifier::without_tags("demo".into()),
            vec![32_000_000, 64_000_000_000],
        );

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
        map.insert(Identifier::without_tags("test".into()), 10);

        assert!(registry.add(Metric::new(
            Identifier::without_tags("test".into()),
            MetricKind::Gauge(GaugeOperation::Modify(10))
        )));

        assert_eq!(map, registry.gauges);

        let mut map = HashMap::default();
        map.insert(Identifier::without_tags("test".into()), -10);

        assert!(registry.add(Metric::new(
            Identifier::without_tags("test".into()),
            MetricKind::Gauge(GaugeOperation::Modify(-20))
        )));

        assert_eq!(map, registry.gauges);

        let mut map = HashMap::default();
        map.insert(Identifier::without_tags("test".into()), 32);

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

    #[test]
    fn counter_statistics_are_correctly_calculated() {
        let mut registry = Registry::default();

        for i in 1..=100 {
            registry.add(Metric::new(
                Identifier::without_tags("test".into()),
                MetricKind::Counter(i),
            ));
        }

        let (metrics, overflowing_metrics) = registry.finalize().unwrap();

        assert!(overflowing_metrics.is_empty());

        let statistics = metrics
            .counters
            .get(&Identifier::without_tags("test".into()))
            .expect("Metric `test` should exist");

        assert_eq!(50, statistics.percentile(50.into()));
        assert_eq!(99, statistics.percentile(99.into()));
        assert_eq!(100, statistics.percentile(100.into()));
        assert_eq!(50, statistics.median());
        assert_eq!(5050, statistics.sum());
    }
}

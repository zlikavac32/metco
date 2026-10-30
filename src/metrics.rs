use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::{self, Display, Formatter};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerResolution {
    Seconds,
    MilliSeconds,
    MicroSeconds,
    NanoSeconds,
}

impl TimerResolution {
    fn nanos_multiplier(self) -> u64 {
        match self {
            TimerResolution::Seconds => 1_000_000_000,
            TimerResolution::MilliSeconds => 1_000_000,
            TimerResolution::MicroSeconds => 1_000,
            TimerResolution::NanoSeconds => 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GaugeOperation {
    Set(i64),
    Modify(i64),
    Remove,
}

#[derive(Debug, PartialEq)]
pub enum MetricKind {
    Counter(u64),
    Histogram(u64),
    Timing(u64, TimerResolution),
    Gauge(GaugeOperation),
}

impl MetricKind {
    pub fn name(&self) -> &'static str {
        match self {
            MetricKind::Counter(_) => "counter",
            MetricKind::Histogram(_) => "histogram",
            MetricKind::Timing(..) => "timer",
            MetricKind::Gauge(_) => "gauge",
        }
    }
}

/// Metric name with its tags.
///
/// Tags are kept sorted so that equal identifiers always hash equally.
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct Identifier {
    name: String,
    tags: BTreeMap<String, String>,
}

impl Identifier {
    #[cfg(test)]
    pub fn without_tags(name: impl Into<String>) -> Self {
        Self::with_tags(name, BTreeMap::new())
    }

    pub fn with_tags(name: impl Into<String>, tags: BTreeMap<String, String>) -> Self {
        Self {
            name: name.into(),
            tags,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn tags(&self) -> &BTreeMap<String, String> {
        &self.tags
    }
}

#[derive(Debug, PartialEq)]
pub struct Metric {
    identifier: Identifier,
    kind: MetricKind,
}

impl Metric {
    pub fn new(identifier: Identifier, kind: MetricKind) -> Self {
        Self { identifier, kind }
    }

    pub fn kind(&self) -> &MetricKind {
        &self.kind
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Percentile(f64);

impl Percentile {
    pub const P50: Self = Self::new(50);
    pub const P75: Self = Self::new(75);
    pub const P90: Self = Self::new(90);
    pub const P99: Self = Self::new(99);

    pub const fn new(value: u32) -> Self {
        assert!(value <= 100, "Percentile out of range [0, 100]");

        Self(value as f64)
    }
}

/// Numeric value of a single statistic, preserving whether it is integral.
#[derive(Debug, Clone, Copy)]
pub enum Number {
    Integer(i64),
    Float(f64),
}

impl From<Number> for f64 {
    fn from(number: Number) -> Self {
        match number {
            Number::Integer(value) => value as f64,
            Number::Float(value) => value,
        }
    }
}

impl Display for Number {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Number::Integer(value) => value.fmt(f),
            Number::Float(value) => value.fmt(f),
        }
    }
}

#[derive(Debug)]
pub struct Statistics {
    list: Vec<i64>,
    sum: i64,
    std: f64,
}

impl Statistics {
    /// Returns `None` if the sum of the values overflows.
    fn new(mut list: Vec<i64>) -> Option<Self> {
        assert!(!list.is_empty());

        list.sort_unstable();

        let sum = list
            .iter()
            .try_fold(0i64, |acc, &item| acc.checked_add(item))?;

        let avg = sum as f64 / list.len() as f64;
        let std = list
            .iter()
            .map(|&item| (item as f64 - avg).powi(2))
            .sum::<f64>()
            .sqrt();

        Some(Self { list, sum, std })
    }

    pub fn sum(&self) -> i64 {
        self.sum
    }

    pub fn count(&self) -> usize {
        self.list.len()
    }

    pub fn min(&self) -> i64 {
        *self
            .list
            .first()
            .expect("We asserted that list is not empty")
    }

    pub fn max(&self) -> i64 {
        *self
            .list
            .last()
            .expect("We asserted that list is not empty")
    }

    pub fn median(&self) -> i64 {
        self.percentile(Percentile::P50)
    }

    pub fn std(&self) -> f64 {
        self.std
    }

    pub fn percentile(&self, p: Percentile) -> i64 {
        self.list[(self.list.len() as f64 * (p.0 / 100. - 0.01)).floor() as usize]
    }

    /// All statistics published by backends, in a stable order.
    pub fn summary(&self) -> [(&'static str, Number); 9] {
        [
            ("count", Number::Integer(self.count() as i64)),
            ("sum", Number::Integer(self.sum())),
            ("std", Number::Float(self.std())),
            ("median", Number::Integer(self.median())),
            ("p75", Number::Integer(self.percentile(Percentile::P75))),
            ("p90", Number::Integer(self.percentile(Percentile::P90))),
            ("p99", Number::Integer(self.percentile(Percentile::P99))),
            ("min", Number::Integer(self.min())),
            ("max", Number::Integer(self.max())),
        ]
    }
}

#[derive(Debug)]
pub struct Gauge {
    pub current: i64,
    pub statistics: Statistics,
}

#[derive(Debug)]
pub struct TimeFrame {
    counters: HashMap<Identifier, i64>,
    histograms: HashMap<Identifier, Statistics>,
    gauges: HashMap<Identifier, Gauge>,
    timings: HashMap<Identifier, Statistics>,
}

impl TimeFrame {
    pub fn counters(&self) -> &HashMap<Identifier, i64> {
        &self.counters
    }

    pub fn histograms(&self) -> &HashMap<Identifier, Statistics> {
        &self.histograms
    }

    pub fn gauges(&self) -> &HashMap<Identifier, Gauge> {
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
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let (kind, identifier) = match self {
            OverflowingMetric::Counter(identifier) => ("counter", identifier),
            OverflowingMetric::Timing(identifier) => ("timing", identifier),
        };

        write!(f, "{kind}: {}", identifier.name)?;

        if !identifier.tags.is_empty() {
            todo!("Tags!");
        }

        Ok(())
    }
}

/// Returned when adding a metric would overflow its value.
#[derive(Debug, PartialEq, Eq)]
pub struct Overflow;

#[derive(Debug, Default, Clone, PartialEq)]
struct GaugeState {
    current: i64,
    history: Vec<i64>,
}

#[derive(Debug, Default)]
pub struct Registry {
    counters: HashMap<Identifier, i64>,
    histograms: HashMap<Identifier, Vec<i64>>,
    gauges: HashMap<Identifier, GaugeState>,
    timings: HashMap<Identifier, Vec<i64>>,
    removed_gauges: HashSet<Identifier>,
}

impl Registry {
    pub fn is_empty(&self) -> bool {
        self.counters.is_empty() && self.gauges.is_empty() && self.timings.is_empty()
    }

    pub fn add(&mut self, metric: Metric) -> Result<(), Overflow> {
        let Metric { identifier, kind } = metric;

        match kind {
            MetricKind::Counter(value) => {
                *self.counters.entry(identifier).or_default() += value as i64;
            }
            MetricKind::Histogram(value) => {
                self.histograms
                    .entry(identifier)
                    .or_default()
                    .push(value as i64);
            }
            MetricKind::Timing(value, resolution) => {
                let nanos = value
                    .checked_mul(resolution.nanos_multiplier())
                    .ok_or(Overflow)?;

                self.timings
                    .entry(identifier)
                    .or_default()
                    .push(nanos as i64);
            }
            MetricKind::Gauge(GaugeOperation::Set(value)) => {
                self.removed_gauges.remove(&identifier);

                let gauge = self.gauges.entry(identifier).or_default();

                gauge.current = value;
                gauge.history.push(value);
            }
            MetricKind::Gauge(GaugeOperation::Modify(value)) => {
                let was_removed = self.removed_gauges.remove(&identifier);
                let gauge = self.gauges.entry(identifier).or_default();

                if was_removed {
                    gauge.current = 0;
                }

                gauge.current = gauge.current.checked_add(value).ok_or(Overflow)?;
                gauge.history.push(gauge.current);
            }
            MetricKind::Gauge(GaugeOperation::Remove) => {
                self.removed_gauges.insert(identifier);
            }
        }

        Ok(())
    }

    /// Creates an empty registry which carries over current values of gauges that were not
    /// removed.
    pub fn new_with_gauges(&self) -> Self {
        let mut gauges = self.gauges.clone();

        gauges.retain(|identifier, _| !self.removed_gauges.contains(identifier));

        for gauge in gauges.values_mut() {
            gauge.history.clear();
            gauge.history.push(gauge.current);
        }

        Self {
            gauges,
            ..Default::default()
        }
    }

    pub fn finalize(self) -> (TimeFrame, Vec<OverflowingMetric>) {
        let mut overflowing_metrics = vec![];

        let gauges = self
            .gauges
            .into_iter()
            .filter_map(|(identifier, GaugeState { current, history })| {
                match Statistics::new(history) {
                    Some(statistics) => Some((
                        identifier,
                        Gauge {
                            current,
                            statistics,
                        },
                    )),
                    None => {
                        overflowing_metrics.push(OverflowingMetric::Counter(identifier));
                        None
                    }
                }
            })
            .collect();

        let histograms = collect_statistics(
            self.histograms,
            OverflowingMetric::Counter,
            &mut overflowing_metrics,
        );

        let timings = collect_statistics(
            self.timings,
            OverflowingMetric::Timing,
            &mut overflowing_metrics,
        );

        let time_frame = TimeFrame {
            counters: self.counters,
            histograms,
            gauges,
            timings,
        };

        (time_frame, overflowing_metrics)
    }
}

fn collect_statistics(
    metrics: HashMap<Identifier, Vec<i64>>,
    overflowing: fn(Identifier) -> OverflowingMetric,
    overflowing_metrics: &mut Vec<OverflowingMetric>,
) -> HashMap<Identifier, Statistics> {
    metrics
        .into_iter()
        .filter_map(|(identifier, list)| match Statistics::new(list) {
            Some(statistics) => Some((identifier, statistics)),
            None => {
                overflowing_metrics.push(overflowing(identifier));
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod test {
    use super::*;

    fn gauge(current: i64, history: Vec<i64>) -> GaugeState {
        GaugeState { current, history }
    }

    #[test]
    fn counter_can_be_added() {
        let mut registry = Registry::default();

        let map = HashMap::from([
            (Identifier::without_tags("test"), vec![2, 7]),
            (Identifier::without_tags("demo"), vec![32]),
        ]);

        assert!(registry
            .add(Metric::new(
                Identifier::without_tags("test"),
                MetricKind::Histogram(2)
            ))
            .is_ok());
        assert!(registry
            .add(Metric::new(
                Identifier::without_tags("demo"),
                MetricKind::Histogram(32)
            ))
            .is_ok());
        assert!(registry
            .add(Metric::new(
                Identifier::without_tags("test"),
                MetricKind::Histogram(7)
            ))
            .is_ok());

        assert_eq!(map, registry.histograms)
    }

    #[test]
    fn metrics_with_same_tags_are_aggregated_together() {
        let mut registry = Registry::default();

        for _ in 0..50 {
            let tags = (0..8)
                .map(|i| (format!("tag{i}"), format!("value{i}")))
                .collect::<HashMap<_, _>>()
                .into_iter()
                .collect();

            registry
                .add(Metric::new(
                    Identifier::with_tags("test", tags),
                    MetricKind::Counter(1),
                ))
                .unwrap();
        }

        assert_eq!(1, registry.counters.len());
        assert_eq!(Some(&50), registry.counters.values().next());
    }

    #[test]
    fn timings_can_be_added() {
        let mut registry = Registry::default();

        let map = HashMap::from([
            (Identifier::without_tags("test"), vec![2, 7_000]),
            (
                Identifier::without_tags("demo"),
                vec![32_000_000, 64_000_000_000],
            ),
        ]);

        assert!(registry
            .add(Metric::new(
                Identifier::without_tags("test"),
                MetricKind::Timing(2, TimerResolution::NanoSeconds)
            ))
            .is_ok());
        assert!(registry
            .add(Metric::new(
                Identifier::without_tags("demo"),
                MetricKind::Timing(32, TimerResolution::MilliSeconds)
            ))
            .is_ok());
        assert!(registry
            .add(Metric::new(
                Identifier::without_tags("test"),
                MetricKind::Timing(7, TimerResolution::MicroSeconds)
            ))
            .is_ok());
        assert!(registry
            .add(Metric::new(
                Identifier::without_tags("demo"),
                MetricKind::Timing(64, TimerResolution::Seconds)
            ))
            .is_ok());

        assert_eq!(map, registry.timings)
    }

    #[test]
    fn gauges_can_be_added() {
        let mut registry = Registry::default();
        let identifier = Identifier::without_tags("test");

        assert!(registry
            .add(Metric::new(
                identifier.clone(),
                MetricKind::Gauge(GaugeOperation::Modify(10))
            ))
            .is_ok());

        assert_eq!(
            HashMap::from([(identifier.clone(), gauge(10, vec![10]))]),
            registry.gauges
        );

        assert!(registry
            .add(Metric::new(
                identifier.clone(),
                MetricKind::Gauge(GaugeOperation::Modify(-20))
            ))
            .is_ok());

        assert_eq!(
            HashMap::from([(identifier.clone(), gauge(-10, vec![10, -10]))]),
            registry.gauges
        );

        assert!(registry
            .add(Metric::new(
                identifier.clone(),
                MetricKind::Gauge(GaugeOperation::Set(32))
            ))
            .is_ok());

        assert_eq!(
            HashMap::from([(identifier.clone(), gauge(32, vec![10, -10, 32]))]),
            registry.gauges
        );

        assert!(registry
            .add(Metric::new(
                identifier.clone(),
                MetricKind::Gauge(GaugeOperation::Remove)
            ))
            .is_ok());

        assert_eq!(HashSet::from([identifier.clone()]), registry.removed_gauges);
        assert_eq!(
            HashMap::from([(identifier, gauge(32, vec![10, -10, 32]))]),
            registry.gauges
        );
    }

    #[test]
    fn counter_statistics_are_correctly_calculated() {
        let mut registry = Registry::default();

        for i in 1..=100 {
            registry
                .add(Metric::new(
                    Identifier::without_tags("test"),
                    MetricKind::Histogram(i),
                ))
                .unwrap();
        }

        let (metrics, overflowing_metrics) = registry.finalize();

        assert!(overflowing_metrics.is_empty());

        let statistics = metrics
            .histograms
            .get(&Identifier::without_tags("test"))
            .expect("Metric `test` should exist");

        assert_eq!(50, statistics.percentile(Percentile::P50));
        assert_eq!(99, statistics.percentile(Percentile::P99));
        assert_eq!(100, statistics.percentile(Percentile::new(100)));
        assert_eq!(50, statistics.median());
        assert_eq!(5050, statistics.sum());
    }

    #[test]
    fn gauge_manipulation_is_correct() {
        let mut registry = Registry::default();
        let identifier = Identifier::without_tags("test");

        registry
            .add(Metric::new(
                identifier.clone(),
                MetricKind::Gauge(GaugeOperation::Modify(1234)),
            ))
            .unwrap();

        let mut registry = {
            let cloned = registry.new_with_gauges();

            let (time_frame, _) = registry.finalize();
            let gauge = &time_frame.gauges[&identifier];

            assert_eq!(1234, gauge.current);
            assert_eq!(1, gauge.statistics.count());

            cloned
        };

        registry
            .add(Metric::new(
                identifier.clone(),
                MetricKind::Gauge(GaugeOperation::Remove),
            ))
            .unwrap();

        let registry = {
            let cloned = registry.new_with_gauges();

            let (time_frame, _) = registry.finalize();
            let gauge = &time_frame.gauges[&identifier];

            assert_eq!(1234, gauge.current);
            assert_eq!(1, gauge.statistics.count());

            cloned
        };

        assert!(registry.gauges.is_empty());
    }
}

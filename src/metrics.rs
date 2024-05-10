use std::collections::HashMap;

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
pub struct Metric {
    pub name: String,
    pub kind: MetricKind,
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

    pub fn average(&self) -> f64 {
        self.sum as f64 / self.list.len() as f64
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
            [((self.list.len() as f64 * p.max(0.).min(1.)).floor() as usize).min(self.list.len())]
    }
}

#[derive(Debug)]
pub struct TimeFrame {
    pub counters: HashMap<String, Statistics>,
    pub gauges: HashMap<String, i64>,
    pub timings: HashMap<String, Statistics>,
}

impl TryFrom<Registry> for TimeFrame {
    type Error = ();

    fn try_from(value: Registry) -> Result<Self, Self::Error> {
        Ok(TimeFrame {
            gauges: value.gauges,
            counters: value.counters.into_iter().fold(
                HashMap::default(),
                |mut map, (name, list)| {
                    if let Ok(statistics) = Statistics::new(list) {
                        map.insert(name, statistics);
                    }

                    map
                },
            ),
            timings: value
                .timings
                .into_iter()
                .fold(HashMap::default(), |mut map, (name, list)| {
                    if let Ok(statistics) = Statistics::new(list) {
                        map.insert(name, statistics);
                    }

                    map
                }),
        })
    }
}

#[derive(Debug, Default)]
pub struct Registry {
    counters: HashMap<String, Vec<u64>>,
    gauges: HashMap<String, i64>,
    timings: HashMap<String, Vec<u64>>,
}

impl Registry {
    pub fn add(&mut self, metric: &Metric) -> bool {
        match &metric.kind {
            MetricKind::Counter(value) => self
                .counters
                .entry(metric.name.clone())
                .or_default()
                .push(*value),
            MetricKind::Timing(value, resolution) => {
                self.timings.entry(metric.name.clone()).or_default().push(
                    value
                        * match resolution {
                            TimerResolution::Seconds => 1_000_000_000,
                            TimerResolution::MilliSeconds => 1_000_000,
                            TimerResolution::MicroSeconds => 1_000,
                            TimerResolution::NanoSeconds => 1,
                        },
                )
            }
            MetricKind::Gauge(operation) => match operation {
                GaugeOperation::Set(value) => {
                    self.gauges.insert(metric.name.clone(), *value);
                }
                GaugeOperation::Modify(value) => {
                    let val = self.gauges.entry(metric.name.clone()).or_default();

                    match val.checked_add(*value) {
                        None => return false,
                        Some(res) => *val = res,
                    }
                }
                GaugeOperation::Remove => {
                    self.gauges.remove(&metric.name);
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

    pub fn finalize(self) -> Option<TimeFrame> {
        TimeFrame::try_from(self).ok()
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

        assert!(registry.add(&Metric {
            name: "test".into(),
            kind: MetricKind::Counter(2)
        }));
        assert!(registry.add(&Metric {
            name: "demo".into(),
            kind: MetricKind::Counter(32)
        }));
        assert!(registry.add(&Metric {
            name: "test".into(),
            kind: MetricKind::Counter(7)
        }));

        assert_eq!(map, registry.counters)
    }

    #[test]
    fn timings_can_be_added() {
        let mut registry = Registry::default();

        let mut map = HashMap::default();
        map.insert("test".into(), vec![2, 7_000]);
        map.insert("demo".into(), vec![32_000_000, 64_000_000_000]);

        assert!(registry.add(&Metric {
            name: "test".into(),
            kind: MetricKind::Timing(2, TimerResolution::NanoSeconds)
        }));
        assert!(registry.add(&Metric {
            name: "demo".into(),
            kind: MetricKind::Timing(32, TimerResolution::MilliSeconds)
        }));
        assert!(registry.add(&Metric {
            name: "test".into(),
            kind: MetricKind::Timing(7, TimerResolution::MicroSeconds)
        }));
        assert!(registry.add(&Metric {
            name: "demo".into(),
            kind: MetricKind::Timing(64, TimerResolution::Seconds)
        }));

        assert_eq!(map, registry.timings)
    }

    #[test]
    fn gauges_can_be_added() {
        let mut registry = Registry::default();

        let mut map = HashMap::default();
        map.insert("test".into(), 10);

        assert!(registry.add(&Metric {
            name: "test".into(),
            kind: MetricKind::Gauge(GaugeOperation::Modify(10))
        }));

        assert_eq!(map, registry.gauges);

        let mut map = HashMap::default();
        map.insert("test".into(), -10);

        assert!(registry.add(&Metric {
            name: "test".into(),
            kind: MetricKind::Gauge(GaugeOperation::Modify(-20))
        }));

        assert_eq!(map, registry.gauges);

        let mut map = HashMap::default();
        map.insert("test".into(), 32);

        assert!(registry.add(&Metric {
            name: "test".into(),
            kind: MetricKind::Gauge(GaugeOperation::Set(32))
        }));

        assert_eq!(map, registry.gauges);

        assert!(registry.add(&Metric {
            name: "test".into(),
            kind: MetricKind::Gauge(GaugeOperation::Remove)
        }));

        assert_eq!(HashMap::default(), registry.gauges);
    }
}

use std::collections::HashMap;
use std::error::Error;
use std::io::ErrorKind;
use std::net::UdpSocket;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use chrono::Utc;
use clap::Parser;
use figment::providers::{Env, Format, Toml};
use figment::Figment;
use serde::Deserialize;
use stderrlog::Timestamp;

use crate::metrics::{Identifier, Metric, MetricKind, Registry, TimerResolution};

mod backend;
mod metrics;
mod protocol;

/// Big enough for any UDP datagram so that payloads are never truncated.
const MAX_DATAGRAM_SIZE: usize = 65_536;

#[derive(Deserialize, Debug)]
#[serde(
    tag = "type",
    rename_all = "lowercase",
    rename_all_fields = "kebab-case"
)]
enum BackendConfig {
    Console,
    PostgreSQL {
        host: String,
        port: u16,
        user: String,
        password: String,
        db_name: String,
    },
    ElasticSearch {
        dsn: String,
        index_format: String,
        accept_invalid_certs: bool,
    },
}

impl BackendConfig {
    fn create(&self) -> Result<Box<dyn backend::Backend>, Box<dyn Error>> {
        Ok(match self {
            BackendConfig::Console => Box::new(backend::Console),
            BackendConfig::PostgreSQL {
                host,
                port,
                user,
                password,
                db_name,
            } => {
                let client = postgres::Config::new()
                    .host(host)
                    .port(*port)
                    .user(user)
                    .password(password)
                    .dbname(db_name)
                    .connect(postgres::NoTls)?;

                Box::new(backend::PostgreSQL::new(client))
            }
            BackendConfig::ElasticSearch {
                dsn,
                index_format,
                accept_invalid_certs,
            } => {
                let client = reqwest::blocking::Client::builder()
                    .danger_accept_invalid_certs(*accept_invalid_certs)
                    .build()?;

                Box::new(backend::ElasticSearch::new(
                    client,
                    dsn.clone(),
                    index_format.clone(),
                ))
            }
        })
    }
}

#[derive(Deserialize, Debug)]
#[serde(try_from = "BackendsRaw")]
struct Backends {
    enabled: Vec<(String, BackendConfig)>,
}

#[derive(Deserialize, Debug)]
struct BackendsRaw {
    enabled: Vec<String>,
    available: HashMap<String, BackendConfig>,
}

impl TryFrom<BackendsRaw> for Backends {
    type Error = String;

    fn try_from(
        BackendsRaw {
            enabled,
            mut available,
        }: BackendsRaw,
    ) -> Result<Self, Self::Error> {
        let enabled = enabled
            .into_iter()
            .map(|backend_name| match available.remove(&backend_name) {
                Some(backend) => Ok((backend_name, backend)),
                None => Err(format!(
                    "Backend {backend_name} listed in enabled is not defined as available or is listed multiple times. \
                    Check if you have [backend.available.{backend_name}] in your config or if you defined same \
                    backend multiple times as enabled."
                )),
            })
            .collect::<Result<_, _>>()?;

        Ok(Backends { enabled })
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
struct Config {
    host: String,
    port: u16,
    #[serde(with = "humantime_serde")]
    refresh_interval: Duration,
    #[serde(rename = "backend")]
    backends: Backends,
}

#[derive(Parser, Debug)]
#[command(version, long_about = None)]
struct Cli {
    #[arg(short, long)]
    quiet: bool,

    #[arg(short, long, default_value_t = 1, action = clap::ArgAction::Count)]
    verbosity: u8,

    #[arg(short, long, default_value = "config.toml")]
    config_path: PathBuf,
}

fn init_logging(cli: &Cli) {
    stderrlog::new()
        .module(module_path!())
        .quiet(cli.quiet)
        .verbosity(usize::from(cli.verbosity))
        .timestamp(Timestamp::Microsecond)
        .init()
        .expect("Unable to initialize logging infrastructure");
}

fn flush_to_backends(config: Arc<Config>, registry: Registry, telemetry: Registry) {
    thread::spawn(move || {
        let backends = config
            .backends
            .enabled
            .iter()
            .filter_map(|(name, backend)| match backend.create() {
                Ok(backend) => Some((name, backend)),
                Err(err) => {
                    log::error!("[{name}] {err}");

                    None
                }
            })
            .collect::<Vec<_>>();

        log::info!("Aggregating collected metrics");

        let now = Utc::now();

        let time_frames = [registry, telemetry]
            .into_iter()
            .map(|registry| {
                let (time_frame, overflowing_metrics) = registry.finalize();

                for overflowing_metric in overflowing_metrics {
                    log::warn!("Overflowing metric {overflowing_metric}");
                }

                time_frame
            })
            .collect::<Vec<_>>();

        for (name, mut backend) in backends {
            log::trace!("Notifying backend {name:?}");

            let logger = backend::Logger::new(name);

            for time_frame in &time_frames {
                backend.publish(&now, time_frame, &logger);
            }
        }
    });
}

struct Collector {
    config: Arc<Config>,
    host: String,
    registry: Registry,
    telemetry: Registry,
}

impl Collector {
    fn new(config: Arc<Config>, host: String) -> Self {
        Self {
            config,
            host,
            registry: Registry::default(),
            telemetry: Registry::default(),
        }
    }

    fn add_telemetry(&mut self, name: &str, extra_tags: &[(&str, &str)], kind: MetricKind) {
        let tags = [("host", self.host.as_str())]
            .iter()
            .chain(extra_tags)
            .map(|&(k, v)| (k.to_owned(), v.to_owned()))
            .collect();

        // Telemetry values are small enough that overflow can be ignored.
        let _ = self
            .telemetry
            .add(Metric::new(Identifier::with_tags(name, tags), kind));
    }

    fn flush(&mut self) {
        if self.registry.is_empty() && self.telemetry.is_empty() {
            log::info!("Registry is empty, nothing to aggregate");

            return;
        }

        let memory_usage = memory_stats::memory_stats()
            .expect("Memory usage should be computed")
            .physical_mem as u64;

        let new_registry = self.registry.new_with_gauges();
        let new_telemetry = self.telemetry.new_with_gauges();
        let registry = std::mem::replace(&mut self.registry, new_registry);

        self.add_telemetry("metco.memory_usage", &[], MetricKind::Counter(memory_usage));

        let telemetry = std::mem::replace(&mut self.telemetry, new_telemetry);

        flush_to_backends(Arc::clone(&self.config), registry, telemetry);
    }

    fn handle_payload(&mut self, payload: &[u8]) {
        let start = Instant::now();

        self.add_telemetry(
            "metco.bytes_read",
            &[],
            MetricKind::Histogram(payload.len() as u64),
        );

        let payload = match std::str::from_utf8(payload) {
            Ok(payload) => payload,
            Err(err) => {
                log::warn!("Unable to handle payload as utf8: {err}");
                log::trace!("Payload: {payload:?}");

                return;
            }
        };

        let (metrics, rest) = protocol::parse_protocol(payload);

        let mut parsed_counts = HashMap::<&'static str, u64>::new();

        for metric in metrics {
            log::trace!("Parsed metric: {metric:?}");

            *parsed_counts.entry(metric.kind().name()).or_default() += 1;

            if self.registry.add(metric).is_err() {
                log::warn!("To big metric received, ignoring");
            }
        }

        if let Some(rest) = rest {
            log::info!("Failed to process bytes: {rest:?}");

            self.add_telemetry(
                "metco.bytes_not_parsed",
                &[],
                MetricKind::Histogram(rest.len() as u64),
            );
        }

        for (kind, count) in parsed_counts {
            self.add_telemetry(
                "metco.metrics_parsed",
                &[("kind", kind)],
                MetricKind::Histogram(count),
            );
        }

        self.add_telemetry(
            "metco.iteration_duration",
            &[],
            MetricKind::Timing(
                start.elapsed().as_nanos() as u64,
                TimerResolution::NanoSeconds,
            ),
        );
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    init_logging(&cli);

    let host = hostname::get()?
        .into_string()
        .map_err(|host| format!("Hostname {host:?} is not valid UTF-8 string"))?;

    let config: Arc<Config> = Arc::new(
        Figment::new()
            .merge(Toml::file(&cli.config_path))
            .merge(Env::prefixed("METCO_"))
            .extract()?,
    );

    if config.refresh_interval.is_zero() {
        return Err("refresh-interval must be greater than zero".into());
    }

    let socket = UdpSocket::bind((config.host.as_str(), config.port))?;

    let refresh_interval = config.refresh_interval;
    let mut collector = Collector::new(config, host);
    let mut last_flush = Instant::now();
    let mut buff = vec![0; MAX_DATAGRAM_SIZE];

    loop {
        let elapsed = last_flush.elapsed();

        if elapsed >= refresh_interval {
            collector.flush();
            last_flush = Instant::now();

            continue;
        }

        socket.set_read_timeout(Some(refresh_interval - elapsed))?;

        log::info!("Waiting for data from socket");

        match socket.recv(&mut buff) {
            Ok(size_read) => collector.handle_payload(&buff[..size_read]),
            // Timeout is reported as `WouldBlock` on Unix and as `TimedOut` on Windows.
            Err(err) if matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                log::trace!("Read timeout reached")
            }
            Err(err) => {
                log::error!("Error occurred: {err}");

                return Err(err.into());
            }
        }
    }
}

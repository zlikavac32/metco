use std::collections::HashMap;
use std::error::Error;
use std::io::ErrorKind;
use std::net::UdpSocket;
use std::path::PathBuf;
use std::process::exit;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use chrono::Utc;
use clap::Parser;
use figment::providers::{Env, Format, Toml};
use figment::Figment;
use serde::Deserialize;
use stderrlog::Timestamp;

use crate::metrics::{GaugeOperation, Identifier, Metric, MetricKind, Registry, TimerResolution};

mod backend;
mod metrics;
mod protocol;

#[derive(Deserialize, Debug)]
#[serde(tag = "type")]
enum Backend {
    #[serde(rename = "console")]
    Console,
    #[serde(rename = "postgresql")]
    PostgreSQL {
        host: String,
        port: u16,
        user: String,
        password: String,
        #[serde(rename = "db-name")]
        db_name: String,
    },
    #[serde(rename = "elasticsearch")]
    ElasticSearch {
        dsn: String,
        #[serde(rename = "index-format")]
        index_format: String,
        #[serde(rename = "accept-invalid-certs")]
        accept_invalid_certs: bool,
    },
}

#[derive(Deserialize, Debug)]
#[serde(try_from = "BackendsRaw")]
struct Backends {
    enabled: Vec<(String, Backend)>,
}

#[derive(Deserialize, Debug)]
struct BackendsRaw {
    enabled: Vec<String>,
    available: HashMap<String, Backend>,
}

impl TryFrom<BackendsRaw> for Backends {
    type Error = String;

    fn try_from(mut backends: BackendsRaw) -> Result<Self, Self::Error> {
        let mut enabled = vec![];

        for backend_name in backends.enabled {
            if !backends.available.contains_key(&backend_name) {
                return Err(format!(
                    "Backend {backend_name} listed in enabled is not defined as available or is listed multiple times. \
                    Check if you have [backend.available.{backend_name}] in your config or if you defined same \
                    backend multiple times as enabled."
                ));
            }

            let backend = backends
                .available
                .remove(&backend_name)
                .expect("We checked that backend exists");

            enabled.push((backend_name, backend));
        }

        Ok(Backends { enabled })
    }
}

#[derive(Deserialize, Debug)]
struct Config {
    host: String,
    port: u16,
    #[serde(rename = "refresh-interval", with = "humantime_serde")]
    refresh_interval: Duration,
    #[serde(rename = "backend")]
    backends: Backends,
}

fn init_logging(cli: &CLI) {
    stderrlog::new()
        .module(module_path!())
        .quiet(cli.quiet)
        .verbosity(cli.verbosity as usize)
        .timestamp(Timestamp::Microsecond)
        .init()
        .expect("Unable to initialize logging infrastructure");
}

#[derive(Parser, Debug)]
#[allow(clippy::upper_case_acronyms)]
#[command(version, long_about = None)]
struct CLI {
    #[arg(short, long)]
    quiet: bool,

    #[arg(short, long, default_value_t = 1, action = clap::ArgAction::Count)]
    verbosity: u8,

    #[arg(short, long, default_value = "config.toml")]
    config_path: PathBuf,
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = CLI::parse();
    init_logging(&cli);

    let host = hostname::get()
        .expect("Unable to load hostname")
        .into_string()
        .expect("Hostname is not valid UTF-8 string");

    let config: Arc<Config> = Arc::new(
        Figment::new()
            .merge(Toml::file(cli.config_path))
            .merge(Env::prefixed("METCO_"))
            .extract()?,
    );

    let socket = UdpSocket::bind(format!("{}:{}", config.host, config.port)).unwrap();

    let mut now = Instant::now();

    fn flush(
        registry: Registry,
        mut telemetry: Registry,
        config: Arc<Config>,
        host: String,
    ) -> (Registry, Registry) {
        if registry.is_empty() {
            log::info!("Registry is empty, nothing to aggregate");

            return (registry, telemetry);
        }

        let new_registry = registry.new_with_gauges();
        let new_telemetry = telemetry.new_with_gauges();

        telemetry.add(Metric::new(
            Identifier::with_tags(
                "metco.memory_usage".into(),
                HashMap::from([("host".into(), host)]),
            ),
            MetricKind::Gauge(GaugeOperation::Set(
                memory_stats::memory_stats()
                    .expect("Memory usage should be computed")
                    .physical_mem as i64,
            )),
        ));

        type CreatedBackend = (String, Box<dyn backend::Backend>);
        type CreateBackendError = (String, Box<dyn Error>);

        thread::spawn(move || {
            let backends = config
                .backends
                .enabled
                .iter()
                .map(
                    |(name, backend)| -> Result<CreatedBackend, CreateBackendError> {
                        Ok((
                            name.clone(),
                            match backend {
                                Backend::Console => Box::<backend::Console>::default(),
                                Backend::PostgreSQL {
                                    host,
                                    port,
                                    user,
                                    password,
                                    db_name,
                                } => Box::new(backend::PostgreSQL::new({
                                    let mut config = postgres::Config::new();

                                    config.host(host);
                                    config.port(*port);
                                    config.user(user);
                                    config.password(password);
                                    config.dbname(db_name);

                                    match config.connect(postgres::NoTls) {
                                        Ok(connection) => connection,
                                        Err(err) => return Err((name.clone(), err.into())),
                                    }
                                })),
                                Backend::ElasticSearch {
                                    dsn,
                                    index_format,
                                    accept_invalid_certs,
                                } => {
                                    let mut builder = reqwest::blocking::Client::builder();

                                    if *accept_invalid_certs {
                                        builder = builder.danger_accept_invalid_certs(true);
                                    }

                                    Box::new(backend::ElasticSearch::new(
                                        match builder.build() {
                                            Ok(client) => client,
                                            Err(err) => return Err((name.clone(), err.into())),
                                        },
                                        dsn.clone(),
                                        index_format.clone(),
                                    ))
                                }
                            },
                        ))
                    },
                )
                .filter_map(|backend_result| match backend_result {
                    Ok(backend) => Some(backend),
                    Err((backend, err)) => {
                        log::error!("[{}] {}", backend, err);

                        None
                    }
                })
                .collect::<Vec<_>>();

            log::info!("Aggregating collected metrics");

            let now = Utc::now();

            let time_frames = [registry, telemetry]
                .into_iter()
                .filter_map(|registry| {
                    if let Some((time_frame, overflowing_metrics)) = registry.finalize() {
                        for overflowing_metric in overflowing_metrics {
                            log::warn!("Overflowing metric {}", overflowing_metric);
                        }

                        Some(time_frame)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();

            for mut backend in backends {
                log::trace!("Notifying backend {:?}", backend.0);

                for time_frame in &time_frames {
                    backend
                        .1
                        .publish(&now, time_frame, backend::Logger::new(backend.0.clone()));
                }
            }
        });

        (new_registry, new_telemetry)
    }

    let mut registry = Registry::default();
    let mut telemetry = Registry::default();

    let mut buff = [0; 2048];

    loop {
        let elapsed = now.elapsed();

        if elapsed >= config.refresh_interval {
            (registry, telemetry) = flush(registry, telemetry, config.clone(), host.clone());
            now = Instant::now();
        } else {
            socket
                .set_read_timeout(Some(config.refresh_interval - elapsed))
                .expect("Why are we unable to set read timeout?");
        }

        log::info!("Waiting for data from socket");

        match socket.recv(&mut buff) {
            Ok(size_read) => {
                let now = Instant::now();

                telemetry.add(Metric::new(
                    Identifier::with_tags(
                        "metco.bytes_read".into(),
                        HashMap::from([("host".into(), host.clone())]),
                    ),
                    MetricKind::Counter(size_read as u64),
                ));

                let payload = match std::str::from_utf8(&buff[..size_read]) {
                    Ok(payload) => payload,
                    Err(err) => {
                        log::warn!("Unable to handle payload as utf8: {}", err);
                        log::trace!("Payload: {:?}", &buff);

                        continue;
                    }
                };

                let metrics = protocol::parse_protocol(payload);

                let mut counters_count = 0;
                let mut timers_count = 0;
                let mut gauges_count = 0;

                for metric in metrics {
                    log::trace!("Parsed metric: {:?}", &metric);

                    if metric.is_counter() {
                        counters_count += 1;
                    } else if metric.is_timer() {
                        timers_count += 1;
                    } else if metric.is_gauge() {
                        gauges_count += 1;
                    }

                    if !registry.add(metric) {
                        log::warn!("To big metric received, ignoring");
                    }
                }

                if counters_count > 0 {
                    telemetry.add(Metric::new(
                        Identifier::with_tags(
                            "metco.metrics_parsed".into(),
                            HashMap::from([("host".into(), host.clone()), ("kind".into(), "counter".into())]),
                        ),
                        MetricKind::Counter(counters_count),
                    ));
                }

                if timers_count > 0 {
                    telemetry.add(Metric::new(
                        Identifier::with_tags(
                            "metco.metrics_parsed".into(),
                            HashMap::from([("host".into(), host.clone()), ("kind".into(), "timer".into())]),
                        ),
                        MetricKind::Counter(timers_count),
                    ));
                }

                if gauges_count > 0 {
                    telemetry.add(Metric::new(
                        Identifier::with_tags(
                            "metco.metrics_parsed".into(),
                            HashMap::from([("host".into(), host.clone()), ("kind".into(), "gauge".into())]),
                        ),
                        MetricKind::Counter(gauges_count),
                    ));
                }

                let elapsed = now.elapsed();

                telemetry.add(Metric::new(
                    Identifier::with_tags(
                        "metco.iteration_duration".into(),
                        HashMap::from([("host".into(), host.clone())]),
                    ),
                    MetricKind::Timing(elapsed.as_nanos() as u64, TimerResolution::NanoSeconds),
                ));
            }
            Err(err) => {
                if err.kind() != ErrorKind::WouldBlock {
                    log::error!("Error occurred: {}", err);

                    exit(1);
                } else {
                    log::trace!("Read timeout reached");
                }
            }
        }
    }
}

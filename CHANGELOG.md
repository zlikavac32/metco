# Changelog

All notable changes to this project will be documented in this file.

## Unreleased

## Fixed

- Wrong ES backend prefix for `min`/`max` timing metric [#28](https://github.com/zlikavac32/metco/pull/28)
- Wrong variable used for internal `histogram` count [#29](https://github.com/zlikavac32/metco/pull/29)

## 0.9.1 - 2025-10-09

## Fixed

- Gauge clears history between iterations [#27](https://github.com/zlikavac32/metco/pull/27)

## 0.9.0 - 2025-10-08

## Changed

- Counter no longer assumes value of 1 [#25](https://github.com/zlikavac32/metco/pull/25)
- Metco memory usage is now sent as `counter` [#26](https://github.com/zlikavac32/metco/pull/26)

## 0.8.0 - 2025-10-07

## Added

- Histogram metric is added and regular counter no longer has an input value [#22](https://github.com/zlikavac32/metco/pull/22)

## Changed

- Removed gauge is persisted until new cycle [#23](https://github.com/zlikavac32/metco/pull/23)
- Backends send full statistics for gauge [#24](https://github.com/zlikavac32/metco/pull/24)

## 0.7.0 - 2025-10-03

### Added

- 99th percentile in supported backends [#20](https://github.com/zlikavac32/metco/pull/20)

### Fixed

- Percentile calculation [#19](https://github.com/zlikavac32/metco/pull/19)

## 0.6.0 - 2025-09-30

### Changed

- Console backend prints tags [#17](https://github.com/zlikavac32/metco/pull/17)

### Added

- Not processed bytes are now tracked [#18](https://github.com/zlikavac32/metco/pull/18)

## 0.5.0 - 2025-08-31

### Changed

- Internal telemetry now uses single counter `metco.metrics_parsed` for parsed metrics [#16](https://github.com/zlikavac32/metco/pull/16)

## 0.4.0 - 2025-08-31

### Added

- Tags are now supported [#15](https://github.com/zlikavac32/metco/pull/15)

### Changed

- Split PostgreSQL `metrics` table into multiple [#14](https://github.com/zlikavac32/metco/pull/14)

### Fixed

- Internal telemetry gauge metric name [#13](https://github.com/zlikavac32/metco/pull/13)

### Removed

- Hostname information is now responsibility of the client and can be sent (if needed) as a tag [#15](https://github.com/zlikavac32/metco/pull/15)

## 0.3.0 - 2025-03-11

### Added

- Internal telemetry [#12](https://github.com/zlikavac32/metco/pull/12)
- Metrics for `min`/`max` [#11](https://github.com/zlikavac32/metco/pull/11)

## 0.2.0 - 2025-03-05

### Added

- Host info in metrics [#5](https://github.com/zlikavac32/metco/pull/5)
- ElasticSearch backend [#1](https://github.com/zlikavac32/metco/pull/1)


## 0.1.0 - 2025-08-16

- First release

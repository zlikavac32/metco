use nom::branch::alt;
use nom::bytes::complete::{escaped_transform, is_not, tag};
use nom::character::complete::{char, digit1};
use nom::combinator::{map, map_res, recognize, value};
use nom::multi::separated_list1;
use nom::sequence::tuple;
use nom::IResult;

use crate::metrics::{GaugeOperation, Metric, MetricKind, TimerResolution};

fn parse_counter(input: &str) -> IResult<&str, MetricKind> {
    let (input, _) = tag("c|")(input)?;

    fn into_u64(input: &str) -> Result<MetricKind, std::num::ParseIntError> {
        Ok(MetricKind::Counter(input.parse::<u64>()?))
    }

    map_res(digit1, into_u64)(input)
}

fn parse_timing(input: &str) -> IResult<&str, MetricKind> {
    let (input, _) = tag("t|")(input)?;

    fn into_u64_timing(input: &str) -> Result<MetricKind, std::num::ParseIntError> {
        Ok(MetricKind::Timing(
            input.parse::<u64>()?,
            TimerResolution::MilliSeconds,
        ))
    }

    fn into_u64(input: &str) -> Result<u64, std::num::ParseIntError> {
        input.parse::<u64>()
    }

    alt((
        map(
            tuple((
                map_res(digit1, into_u64),
                char('|'),
                alt((
                    value(TimerResolution::NanoSeconds, tag("ns")),
                    value(TimerResolution::MilliSeconds, tag("ms")),
                    value(TimerResolution::MicroSeconds, tag("us")),
                    value(TimerResolution::Seconds, tag("s")),
                )),
            )),
            |(value, _, resolution)| MetricKind::Timing(value, resolution),
        ),
        map_res(digit1, into_u64_timing),
    ))(input)
}

fn parse_gauge(input: &str) -> IResult<&str, MetricKind> {
    let (input, _) = tag("g|")(input)?;

    fn into_i64_set(input: &str) -> Result<GaugeOperation, std::num::ParseIntError> {
        Ok(GaugeOperation::Set(input.parse::<i64>()?))
    }

    fn into_i64(input: &str) -> Result<i64, std::num::ParseIntError> {
        input.parse::<i64>()
    }

    map(
        alt((
            map(char('x'), |_| GaugeOperation::Remove),
            map_res(
                alt((recognize(tuple((tag("-"), digit1))), digit1)),
                into_i64_set,
            ),
            map(
                tuple((
                    alt((char('+'), char('-'))),
                    char('='),
                    map_res(digit1, into_i64),
                )),
                |(kind, _, value)| {
                    GaugeOperation::Modify(match kind {
                        '+' => value,
                        '-' => -value,
                        _ => unreachable!("Should be covered by grammar"),
                    })
                },
            ),
        )),
        MetricKind::Gauge,
    )(input)
}

fn parse_kind(input: &str) -> IResult<&str, MetricKind> {
    alt((parse_counter, parse_timing, parse_gauge))(input)
}

fn parse_metric(input: &str) -> IResult<&str, Metric> {
    let (input, name) = escaped_transform(
        is_not("|\\"),
        '\\',
        alt((value("\\", tag("\\")), value("|", tag("|")))),
    )(input)?;

    let (input, _) = char('|')(input)?;

    let (input, kind) = parse_kind(input)?;

    Ok((input, Metric { name, kind }))
}

pub fn parse_protocol(input: &str) -> Vec<Metric> {
    separated_list1(char('\n'), parse_metric)(input).map_or_else(|_| vec![], |(_, metrics)| metrics)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn counter_can_be_parsed() {
        assert_eq!(
            vec![Metric {
                name: "abc".to_string(),
                kind: MetricKind::Counter(12),
            }],
            parse_protocol("abc|c|12")
        );
    }

    #[test]
    fn counter_with_escaped_chars_can_be_parsed() {
        assert_eq!(
            vec![Metric {
                name: "a\\b|c".to_string(),
                kind: MetricKind::Counter(12),
            }],
            parse_protocol("a\\\\b\\|c|c|12")
        );
    }

    #[test]
    fn counter_with_very_big_number_is_not_parsed_but_does_not_crash_program() {
        assert!(parse_protocol(
            "abc|c|123456789123456789123456789123456789123456789123456789123456789123456789"
        )
        .is_empty());
    }

    #[test]
    fn gauge_can_be_parsed() {
        assert_eq!(
            vec![Metric {
                name: "abc".to_string(),
                kind: MetricKind::Gauge(GaugeOperation::Set(12)),
            }],
            parse_protocol("abc|g|12")
        );

        assert_eq!(
            vec![Metric {
                name: "abc".to_string(),
                kind: MetricKind::Gauge(GaugeOperation::Set(-12)),
            }],
            parse_protocol("abc|g|-12")
        );

        assert_eq!(
            vec![Metric {
                name: "abc".to_string(),
                kind: MetricKind::Gauge(GaugeOperation::Modify(12)),
            }],
            parse_protocol("abc|g|+=12")
        );

        assert_eq!(
            vec![Metric {
                name: "abc".to_string(),
                kind: MetricKind::Gauge(GaugeOperation::Modify(-12)),
            }],
            parse_protocol("abc|g|-=12")
        );

        assert_eq!(
            vec![Metric {
                name: "abc".to_string(),
                kind: MetricKind::Gauge(GaugeOperation::Remove),
            }],
            parse_protocol("abc|g|x")
        );
    }

    #[test]
    fn gauge_with_very_big_number_is_not_parsed_but_does_not_crash_program() {
        assert!(parse_protocol(
            "abc|g|123456789123456789123456789123456789123456789123456789123456789123456789"
        )
        .is_empty());

        assert!(parse_protocol(
            "abc|g|+=123456789123456789123456789123456789123456789123456789123456789123456789"
        )
        .is_empty());
    }

    #[test]
    fn timer_can_be_parsed() {
        assert_eq!(
            vec![Metric {
                name: "abc".to_string(),
                kind: MetricKind::Timing(123, TimerResolution::MilliSeconds),
            }],
            parse_protocol("abc|t|123")
        );

        assert_eq!(
            vec![Metric {
                name: "abc".to_string(),
                kind: MetricKind::Timing(123, TimerResolution::MilliSeconds),
            }],
            parse_protocol("abc|t|123|ms")
        );

        assert_eq!(
            vec![Metric {
                name: "abc".to_string(),
                kind: MetricKind::Timing(123, TimerResolution::Seconds),
            }],
            parse_protocol("abc|t|123|s")
        );

        assert_eq!(
            vec![Metric {
                name: "abc".to_string(),
                kind: MetricKind::Timing(123, TimerResolution::MicroSeconds),
            }],
            parse_protocol("abc|t|123|us")
        );

        assert_eq!(
            vec![Metric {
                name: "abc".to_string(),
                kind: MetricKind::Timing(123, TimerResolution::NanoSeconds),
            }],
            parse_protocol("abc|t|123|ns")
        );
    }

    #[test]
    fn timer_with_very_big_number_is_not_parsed_but_does_not_crash_program() {
        assert!(parse_protocol(
            "abc|t|123456789123456789123456789123456789123456789123456789123456789123456789"
        )
        .is_empty());

        assert!(parse_protocol(
            "abc|t|123456789123456789123456789123456789123456789123456789123456789123456789|s"
        )
        .is_empty());

        assert!(parse_protocol("abc|t|18446744073709551616|ns").is_empty());
    }
}

use crate::metrics::{GaugeOperation, Identifier, Metric, MetricKind, TimerResolution};
use nom::branch::alt;
use nom::bytes::complete::{escaped_transform, is_not, tag};
use nom::character::complete::{char, digit1};
use nom::combinator::{map, map_res, opt, recognize, value};
use nom::error::ErrorKind;
use nom::multi::separated_list1;
use nom::sequence::tuple;
use nom::IResult;
use std::collections::HashMap;

fn parse_counter(input: &str) -> IResult<&str, MetricKind> {
    let (input, _) = tag("c|")(input)?;

    fn into_u64(input: &str) -> Result<MetricKind, std::num::ParseIntError> {
        Ok(MetricKind::Counter(input.parse::<u64>()?))
    }

    map_res(digit1, into_u64)(input)
}

fn parse_histogram(input: &str) -> IResult<&str, MetricKind> {
    let (input, _) = tag("h|")(input)?;

    fn into_u64(input: &str) -> Result<MetricKind, std::num::ParseIntError> {
        Ok(MetricKind::Histogram(input.parse::<u64>()?))
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
    alt((parse_counter, parse_histogram, parse_timing, parse_gauge))(input)
}

fn parse_identifier(input: &str) -> IResult<&str, Identifier> {
    let (input, name) = escaped_transform(
        is_not("|\\;"),
        '\\',
        alt((
            value("\\", tag("\\")),
            value("|", tag("|")),
            value(";", tag(";")),
        )),
    )(input)?;

    let (input, tags) = opt(map(
        tuple((
            tag(";"),
            separated_list1(
                tag(";"),
                map(
                    tuple((
                        escaped_transform(
                            is_not("\\="),
                            '\\',
                            alt((value("\\", tag("\\")), value("=", tag("=")))),
                        ),
                        tag("="),
                        escaped_transform(
                            is_not("|\\;"),
                            '\\',
                            alt((
                                value("\\", tag("\\")),
                                value(";", tag(";")),
                                value("|", tag("|")),
                            )),
                        ),
                    )),
                    |(name, _, value)| (name, value),
                ),
            ),
        )),
        |(_, tags)| tags,
    ))(input)?;

    Ok((
        input,
        match tags {
            None => Identifier::without_tags(name),
            Some(tags) => Identifier::with_tags(name, HashMap::from_iter(tags)),
        },
    ))
}

fn parse_metric(original: &str) -> IResult<&str, Metric> {
    let input = original;

    let (input, identifier) = parse_identifier(input)
        .map_err(|_| nom::Err::Error(nom::error::Error::new(original, ErrorKind::Fail)))?;

    let (input, _) = char('|')(input).map_err(|_: nom::Err<nom::error::Error<_>>| {
        nom::Err::Error(nom::error::Error::new(original, ErrorKind::Fail))
    })?;

    let (input, kind) = parse_kind(input)
        .map_err(|_| nom::Err::Error(nom::error::Error::new(original, ErrorKind::Fail)))?;

    Ok((input, Metric::new(identifier, kind)))
}

pub fn parse_protocol(input: &str) -> (Vec<Metric>, Option<String>) {
    match separated_list1(char('\n'), parse_metric)(input) {
        Ok((input, metrics)) => (
            metrics,
            if input.is_empty() {
                None
            } else {
                Some(input.to_string())
            },
        ),
        Err(_) => (vec![], Some(input.to_string())),
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn metric_identifier_with_tags_can_be_parsed() {
        assert_eq!(
            Ok((
                "|c",
                Identifier::with_tags(
                    "abc".into(),
                    HashMap::from([
                        ("tag1".into(), "value;123".into()),
                        ("tag=2".into(), "te=st3".into())
                    ])
                )
            )),
            parse_identifier("abc;tag1=value\\;123;tag\\=2=te=st3|c")
        );
    }

    #[test]
    fn counter_can_be_parsed() {
        assert_eq!(
            (
                vec![Metric::new(
                    Identifier::without_tags("abc".to_string()),
                    MetricKind::Counter(12),
                )],
                None
            ),
            parse_protocol("abc|c|12")
        );
    }

    #[test]
    fn counter_with_escaped_chars_can_be_parsed() {
        assert_eq!(
            (
                vec![Metric::new(
                    Identifier::without_tags("a\\b|c;".to_string()),
                    MetricKind::Counter(55),
                )],
                None,
            ),
            parse_protocol("a\\\\b\\|c\\;|c|55")
        );
    }

    #[test]
    fn histogram_can_be_parsed() {
        assert_eq!(
            (
                vec![Metric::new(
                    Identifier::without_tags("abc".to_string()),
                    MetricKind::Histogram(12),
                )],
                None
            ),
            parse_protocol("abc|h|12")
        );
    }

    #[test]
    fn histogram_with_escaped_chars_can_be_parsed() {
        assert_eq!(
            (
                vec![Metric::new(
                    Identifier::without_tags("a\\b|c;".to_string()),
                    MetricKind::Histogram(12),
                )],
                None,
            ),
            parse_protocol("a\\\\b\\|c\\;|h|12")
        );
    }

    #[test]
    fn histogram_with_very_big_number_is_not_parsed_but_does_not_crash_program() {
        assert_eq!(
            (vec![], Some("abc|h|123456789123456789123456789123456789123456789123456789123456789123456789".into())),
            parse_protocol(
            "abc|h|123456789123456789123456789123456789123456789123456789123456789123456789"
        )
        );
    }

    #[test]
    fn gauge_can_be_parsed() {
        assert_eq!(
            (
                vec![Metric::new(
                    Identifier::without_tags("abc".to_string()),
                    MetricKind::Gauge(GaugeOperation::Set(12)),
                )],
                None,
            ),
            parse_protocol("abc|g|12")
        );

        assert_eq!(
            (
                vec![Metric::new(
                    Identifier::without_tags("abc".to_string()),
                    MetricKind::Gauge(GaugeOperation::Set(-12)),
                )],
                None,
            ),
            parse_protocol("abc|g|-12")
        );

        assert_eq!(
            (
                vec![Metric::new(
                    Identifier::without_tags("abc".to_string()),
                    MetricKind::Gauge(GaugeOperation::Modify(12)),
                )],
                None,
            ),
            parse_protocol("abc|g|+=12")
        );

        assert_eq!(
            (
                vec![Metric::new(
                    Identifier::without_tags("abc".to_string()),
                    MetricKind::Gauge(GaugeOperation::Modify(-12)),
                )],
                None,
            ),
            parse_protocol("abc|g|-=12")
        );

        assert_eq!(
            (
                vec![Metric::new(
                    Identifier::without_tags("abc".to_string()),
                    MetricKind::Gauge(GaugeOperation::Remove),
                )],
                None,
            ),
            parse_protocol("abc|g|x")
        );
    }

    #[test]
    fn gauge_with_very_big_number_is_not_parsed_but_does_not_crash_program() {
        assert_eq!(
            (
                vec![],
                Some("abc|g|123456789123456789123456789123456789123456789123456789123456789123456789".into())
            ),
            parse_protocol(
            "abc|g|123456789123456789123456789123456789123456789123456789123456789123456789"
        )
        );

        assert_eq!(
            (
                vec!{},
                Some("abc|g|+=123456789123456789123456789123456789123456789123456789123456789123456789".into())
                ),
            parse_protocol(
            "abc|g|+=123456789123456789123456789123456789123456789123456789123456789123456789"
        )
        );
    }

    #[test]
    fn timer_can_be_parsed() {
        assert_eq!(
            (
                vec![Metric::new(
                    Identifier::without_tags("abc".to_string()),
                    MetricKind::Timing(123, TimerResolution::MilliSeconds),
                )],
                None,
            ),
            parse_protocol("abc|t|123")
        );

        assert_eq!(
            (
                vec![Metric::new(
                    Identifier::without_tags("abc".to_string()),
                    MetricKind::Timing(123, TimerResolution::MilliSeconds),
                )],
                None,
            ),
            parse_protocol("abc|t|123|ms")
        );

        assert_eq!(
            (
                vec![Metric::new(
                    Identifier::without_tags("abc".to_string()),
                    MetricKind::Timing(123, TimerResolution::Seconds),
                )],
                None,
            ),
            parse_protocol("abc|t|123|s")
        );

        assert_eq!(
            (
                vec![Metric::new(
                    Identifier::without_tags("abc".to_string()),
                    MetricKind::Timing(123, TimerResolution::MicroSeconds),
                )],
                None,
            ),
            parse_protocol("abc|t|123|us")
        );

        assert_eq!(
            (
                vec![Metric::new(
                    Identifier::without_tags("abc".to_string()),
                    MetricKind::Timing(123, TimerResolution::NanoSeconds),
                )],
                None,
            ),
            parse_protocol("abc|t|123|ns")
        );
    }

    #[test]
    fn timer_with_very_big_number_is_not_parsed_but_does_not_crash_program() {
        assert_eq!(
            (vec![], Some("abc|t|123456789123456789123456789123456789123456789123456789123456789123456789".into())),
            parse_protocol(
            "abc|t|123456789123456789123456789123456789123456789123456789123456789123456789"
        )
        );

        assert_eq!(
            (vec![], Some("abc|t|123456789123456789123456789123456789123456789123456789123456789123456789|s".into())),
            parse_protocol(
                "abc|t|123456789123456789123456789123456789123456789123456789123456789123456789|s"
            )
        );

        assert_eq!(
            (vec![], Some("abc|t|18446744073709551616|ns".into())),
            parse_protocol("abc|t|18446744073709551616|ns")
        );
    }
}

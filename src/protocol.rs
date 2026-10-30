use crate::metrics::{GaugeOperation, Identifier, Metric, MetricKind, TimerResolution};
use nom::branch::alt;
use nom::bytes::complete::{escaped_transform, is_not, tag};
use nom::character::complete::{char, digit1, one_of, u64};
use nom::combinator::{map, map_res, opt, recognize, value};
use nom::error::{Error, ErrorKind};
use nom::multi::separated_list1;
use nom::sequence::{pair, preceded, separated_pair, tuple};
use nom::IResult;

fn parse_counter(input: &str) -> IResult<&str, MetricKind> {
    map(preceded(tag("c|"), u64), MetricKind::Counter)(input)
}

fn parse_histogram(input: &str) -> IResult<&str, MetricKind> {
    map(preceded(tag("h|"), u64), MetricKind::Histogram)(input)
}

fn parse_timer_resolution(input: &str) -> IResult<&str, TimerResolution> {
    alt((
        value(TimerResolution::NanoSeconds, tag("ns")),
        value(TimerResolution::MilliSeconds, tag("ms")),
        value(TimerResolution::MicroSeconds, tag("us")),
        value(TimerResolution::Seconds, tag("s")),
    ))(input)
}

fn parse_timing(input: &str) -> IResult<&str, MetricKind> {
    let (input, _) = tag("t|")(input)?;

    let (input, value) = u64(input)?;
    let (input, resolution) = opt(preceded(char('|'), parse_timer_resolution))(input)?;

    Ok((
        input,
        MetricKind::Timing(value, resolution.unwrap_or(TimerResolution::MilliSeconds)),
    ))
}

fn parse_gauge(input: &str) -> IResult<&str, MetricKind> {
    let set = map_res(recognize(pair(opt(char('-')), digit1)), |value: &str| {
        value.parse().map(GaugeOperation::Set)
    });

    let modify = map(
        separated_pair(one_of("+-"), char('='), map_res(digit1, str::parse::<i64>)),
        |(sign, value)| GaugeOperation::Modify(if sign == '-' { -value } else { value }),
    );

    let remove = value(GaugeOperation::Remove, char('x'));

    map(
        preceded(tag("g|"), alt((remove, set, modify))),
        MetricKind::Gauge,
    )(input)
}

fn parse_kind(input: &str) -> IResult<&str, MetricKind> {
    alt((parse_counter, parse_histogram, parse_timing, parse_gauge))(input)
}

fn parse_name(input: &str) -> IResult<&str, String> {
    escaped_transform(
        is_not("|\\;"),
        '\\',
        alt((
            value("\\", tag("\\")),
            value("|", tag("|")),
            value(";", tag(";")),
        )),
    )(input)
}

fn parse_tag_name(input: &str) -> IResult<&str, String> {
    escaped_transform(
        is_not("\\="),
        '\\',
        alt((value("\\", tag("\\")), value("=", tag("=")))),
    )(input)
}

fn parse_identifier(input: &str) -> IResult<&str, Identifier> {
    let (input, name) = parse_name(input)?;

    let (input, tags) = opt(preceded(
        char(';'),
        separated_list1(
            char(';'),
            separated_pair(parse_tag_name, char('='), parse_name),
        ),
    ))(input)?;

    let tags = tags
        .map(|tags| tags.into_iter().collect())
        .unwrap_or_default();

    Ok((input, Identifier::with_tags(name, tags)))
}

fn parse_metric(original: &str) -> IResult<&str, Metric> {
    let (input, (identifier, _, kind)) = tuple((parse_identifier, char('|'), parse_kind))(original)
        .map_err(|_| nom::Err::Error(Error::new(original, ErrorKind::Fail)))?;

    Ok((input, Metric::new(identifier, kind)))
}

/// Parses all metrics from `input`, returning them along with any input that could not be parsed.
pub fn parse_protocol(input: &str) -> (Vec<Metric>, Option<&str>) {
    match separated_list1(char('\n'), parse_metric)(input) {
        Ok((rest, metrics)) => (metrics, (!rest.is_empty()).then_some(rest)),
        Err(_) => (vec![], Some(input)),
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
                    "abc",
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
                    Identifier::without_tags("abc"),
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
                    Identifier::without_tags("a\\b|c;"),
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
                    Identifier::without_tags("abc"),
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
                    Identifier::without_tags("a\\b|c;"),
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
            (vec![], Some("abc|h|123456789123456789123456789123456789123456789123456789123456789123456789")),
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
                    Identifier::without_tags("abc"),
                    MetricKind::Gauge(GaugeOperation::Set(12)),
                )],
                None,
            ),
            parse_protocol("abc|g|12")
        );

        assert_eq!(
            (
                vec![Metric::new(
                    Identifier::without_tags("abc"),
                    MetricKind::Gauge(GaugeOperation::Set(-12)),
                )],
                None,
            ),
            parse_protocol("abc|g|-12")
        );

        assert_eq!(
            (
                vec![Metric::new(
                    Identifier::without_tags("abc"),
                    MetricKind::Gauge(GaugeOperation::Modify(12)),
                )],
                None,
            ),
            parse_protocol("abc|g|+=12")
        );

        assert_eq!(
            (
                vec![Metric::new(
                    Identifier::without_tags("abc"),
                    MetricKind::Gauge(GaugeOperation::Modify(-12)),
                )],
                None,
            ),
            parse_protocol("abc|g|-=12")
        );

        assert_eq!(
            (
                vec![Metric::new(
                    Identifier::without_tags("abc"),
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
                Some("abc|g|123456789123456789123456789123456789123456789123456789123456789123456789")
            ),
            parse_protocol(
            "abc|g|123456789123456789123456789123456789123456789123456789123456789123456789"
        )
        );

        assert_eq!(
            (
                vec![],
                Some("abc|g|+=123456789123456789123456789123456789123456789123456789123456789123456789")
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
                    Identifier::without_tags("abc"),
                    MetricKind::Timing(123, TimerResolution::MilliSeconds),
                )],
                None,
            ),
            parse_protocol("abc|t|123")
        );

        assert_eq!(
            (
                vec![Metric::new(
                    Identifier::without_tags("abc"),
                    MetricKind::Timing(123, TimerResolution::MilliSeconds),
                )],
                None,
            ),
            parse_protocol("abc|t|123|ms")
        );

        assert_eq!(
            (
                vec![Metric::new(
                    Identifier::without_tags("abc"),
                    MetricKind::Timing(123, TimerResolution::Seconds),
                )],
                None,
            ),
            parse_protocol("abc|t|123|s")
        );

        assert_eq!(
            (
                vec![Metric::new(
                    Identifier::without_tags("abc"),
                    MetricKind::Timing(123, TimerResolution::MicroSeconds),
                )],
                None,
            ),
            parse_protocol("abc|t|123|us")
        );

        assert_eq!(
            (
                vec![Metric::new(
                    Identifier::without_tags("abc"),
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
            (vec![], Some("abc|t|123456789123456789123456789123456789123456789123456789123456789123456789")),
            parse_protocol(
            "abc|t|123456789123456789123456789123456789123456789123456789123456789123456789"
        )
        );

        assert_eq!(
            (vec![], Some("abc|t|123456789123456789123456789123456789123456789123456789123456789123456789|s")),
            parse_protocol(
                "abc|t|123456789123456789123456789123456789123456789123456789123456789123456789|s"
            )
        );

        assert_eq!(
            (vec![], Some("abc|t|18446744073709551616|ns")),
            parse_protocol("abc|t|18446744073709551616|ns")
        );
    }
}

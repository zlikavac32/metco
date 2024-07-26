# MetCo

Metrics Collector inspired by StatsD.

Additional/removed features compared to StatsD are:

- support for additional time units like `seconds`, `microseconds` and `nanoseconds`,
- gauge removal,
- setting gauge to negative value without first setting it to zero,
- sets are not supported and
- no sampling interval.

## Disclaimer

I'm still learning Rust, and although I run this in production for two of my clients without any issue, I can not advise you to do the same until you've checked my source code. I believe it's quite stable, but me being a professional I can not guarantee that this could not cause some issue for you.

That being said, I'm looking forward to more experienced Rust developers suggesting improvements and helping me to get better in Rust.

Also, I do plan to keep this as simple as possible because I like Linux philosophy "Do one thing and do it well".

## Protocol

Protocol is simple text based protocol. Metrics are in form of `name|type|value` and optionally some additional fields separated by `|` depending on metric type.

Metric name is any valid UTF-8 sequence of at least one byte. It's backends job to sanitize name if needed. Pipe character and backslash can be escaped using backslash.

Type can be any of `c`, `t` or `g`.

Value format and optional additional fields are defined by the metric type.

Multiple metrics can be sent separated by `\n`. Data parsed before a parsing error occurs is still considered valid, but remaining data is not parsed.

### Counters

Value is always a positive natural number.

```
abc|c|1234
```

### Timers

By default resolution is milliseconds.

Value is always a positive natural number

```
abc|t|1234
```

Supported resolutions are `s`, `ms`, `us` and `ns`.

```
abc|t|1234|ms
```

### Gauges

Value is a natural number.

```
abc|g|123
```

```
abc|g|-123
```

Instead of setting value, it can be updated by incrementing/decrementing by a specified amount.

```
abc|g|+=123
```

```
abc|g|-=123
```

Gauge can be removed by sending `x` as a value.

```
abc|g|x
```

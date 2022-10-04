log4rs Dynamic Filters
========================

This crate provides filters for `log4rs` that can be dynamically controlled at runtime.

Without this crate, `log4rs` can be configured in one of two ways:
1. Programmatically constructing the `Config`
2. Reading from a `.yaml` file

The former is verbose and inflexible; initialising the logger in this way does however give you a `Handle` that can later be used to replace the configuration.
The latter is simple and flexible, even allowing the configuration to be updated during runtime without restarting the application.
However, using the config file means that you cannot adjust the config programmatically at all (short of programmatically rewriting the config file, which is far from an ideal solution).
There is no way to obtain a `Handle`, and even if you did, what would happen if the file was modified?
How would you integrate changes from both sources?

This crate provides the best of both worlds in the form of dynamic filters: filters that can be specified (with default values) in the config file like anything else, but also modified programmatically.
Changing the default value of a dynamic filter in the config file will have no effect on an already-running application.

This crate currently provides one dynamic filter: `DynamicLevelFilter`, the dynamic equivalent of `ThresholdFilter`.

## Example usage
log4rs.yaml:
```yaml
refresh_rate: 1 minute

appenders:
  my_appender:
    kind: console
    filters:
      - kind: dynamic_level
        name: my_dynamic_filter
        default: info

root:
  level: trace
  appenders:
    - my_appender
```
main.rs:
```rust
use log::{info, LevelFilter};
use log4rs_dynamic_filters::{default_deserializers, DynamicLevelFilter};

fn main() {
    log4rs::init_file("log4rs.yaml", default_deserializers())
        .expect("Failed to initialise logging");
    
    info!("This message will be accepted");
    DynamicLevelFilter::set("my_dynamic_filter", LevelFilter::Warn);
    info!("This message will be rejected by the filter");
}
```

## License
Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this crate by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

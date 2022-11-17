#![allow(clippy::needless_doctest_main)]
//! This crate provides filters for [`log4rs`] that can be dynamically controlled at runtime.
//!
//! Without this crate, [`log4rs`] can be configured in one of two ways:
//! 1. Programmatically constructing the [`Config`](log4rs::Config)
//! 2. Reading from a `.yaml` [config file](log4rs::init_file)
//!
//! The former is verbose and inflexible; initialising the logger in this way
//! does however give you a [`Handle`](log4rs::Handle) that can later be used
//! to replace the configuration.
//!
//! The latter is simple and flexible, even allowing the configuration to be
//! updated during runtime without restarting the application. However, using the
//! config file means that you cannot adjust the config programmatically at all
//! (short of programmatically rewriting the config file, which is far from an
//! ideal solution). There is no way to obtain a [`Handle`](log4rs::Handle),
//! and even if you did, what would happen if the file was modified? How would
//! you integrate changes from both sources?
//!
//! This crate provides the best of both worlds in the form of dynamic filters:
//! filters that can be specified (with default values) in the config file like
//! anything else, but also modified programmatically. Changing the default value
//! of a dynamic filter in the config file will have no effect on an
//! already-running application.
//!
//! This crate currently provides one dynamic filter: [`DynamicLevelFilter`],
//! the dynamic equivalent of `ThresholdFilter`.
//!
//! # Example usage
//! log4rs.yaml:
//! ```yaml
//! refresh_rate: 1 minute
//!
//! appenders:
//!   my_appender:
//!     kind: console
//!     filters:
//!       - kind: dynamic_level
//!         name: my_dynamic_filter
//!         default: info
//!
//! root:
//!   level: trace
//!   appenders:
//!     - my_appender
//! ```
//! main.rs:
//! ```no_run
//! use log::{info, LevelFilter};
//! use log4rs_dynamic_filters::{default_deserializers, DynamicLevelFilter};
//!
//! fn main() {
//!     log4rs::init_file("log4rs.yaml", default_deserializers())
//!         .expect("Failed to initialise logging");
//!
//!     info!("This message will be accepted");
//!     DynamicLevelFilter::set("my_dynamic_filter", LevelFilter::Warn);
//!     info!("This message will be rejected by the filter");
//! }
//! ```

use lazy_static::lazy_static;
use log::{LevelFilter, Record};
use log4rs::{
    config::{Deserialize, Deserializers},
    filter::{Filter, Response},
};
use std::collections::HashMap;
use std::sync::RwLock;

/// Get the default deserializers plus the ones from this module.
pub fn default_deserializers() -> Deserializers {
    let mut ds = Deserializers::default();
    add_deserializers(&mut ds);
    ds
}

/// Add this module's deserializers to the given [`Deserializers`].
pub fn add_deserializers(ds: &mut Deserializers) {
    ds.insert("dynamic_level", DynamicLevelFilterDeserializer);
}

lazy_static! {
    /// Global map of all dynamic level filters.
    static ref DYNAMIC_LEVEL_FILTERS: RwLock<HashMap<String, LevelFilter>> = RwLock::default();
}

/// A filter based on the log level that can be programmatically re-configured at runtime.
/// # Configuration
/// ```yaml
/// kind: dynamic_level
/// # The unique name used to configure this filter at runtime.
/// name: foo
/// # The initial log level of the filter.
/// default: warn
/// ```
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DynamicLevelFilter {
    name: String,
}

impl DynamicLevelFilter {
    /// Create a [`DynamicLevelFilter`] with the given name. If that name is unused,
    /// register it and set its level to the given `starting_level`.
    pub fn new(name: String, starting_level: LevelFilter) -> Self {
        let mut filters = DYNAMIC_LEVEL_FILTERS.write().unwrap();
        if !filters.contains_key(&name) {
            let result = filters.insert(name.clone(), starting_level);
            debug_assert!(result.is_none());
        }

        DynamicLevelFilter { name }
    }

    /// Set the [`DynamicLevelFilter`] with the given name to the given level.
    /// Has no effect if the name is not registered.
    pub fn set(name: &str, level: LevelFilter) {
        let mut filters = DYNAMIC_LEVEL_FILTERS.write().unwrap();
        if let Some(filter) = filters.get_mut(name) {
            *filter = level;
        }
    }
}

impl Filter for DynamicLevelFilter {
    fn filter(&self, record: &Record) -> Response {
        let level: LevelFilter = *DYNAMIC_LEVEL_FILTERS
            .read()
            .unwrap()
            .get(&self.name)
            .unwrap();
        if record.level() > level {
            Response::Reject
        } else {
            Response::Neutral
        }
    }
}

/// Configure a [`DynamicLevelFilter`] from a config file.
#[derive(Debug, serde::Deserialize)]
struct DynamicLevelFilterConfig {
    name: String,
    default: LevelFilter,
}

/// Deserialize a [`DynamicLevelFilterConfig`] into a [`DynamicLevelFilter`].
#[derive(Debug)]
struct DynamicLevelFilterDeserializer;

impl Deserialize for DynamicLevelFilterDeserializer {
    type Trait = dyn Filter;
    type Config = DynamicLevelFilterConfig;

    fn deserialize(
        &self,
        config: Self::Config,
        _: &Deserializers,
    ) -> anyhow::Result<Box<Self::Trait>> {
        Ok(Box::new(DynamicLevelFilter::new(
            config.name,
            config.default,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use log::{debug, error, info, trace, warn};
    use log4rs::{
        config::{Appender, Root},
        encode::{pattern::PatternEncoder, Encode},
        Config,
    };
    use log4rs_test_utils::log_testing::{logging_test_setup, LogsHandle, MockAppender};
    use std::sync::MutexGuard;

    fn test_setup(filter: Box<dyn Filter>) -> (MutexGuard<'static, ()>, LogsHandle) {
        const APPENDER_NAME: &str = "mock";
        let encoder: Box<dyn Encode> = Box::new(PatternEncoder::new("{l} {m}"));
        let (mock, logs) = MockAppender::new(encoder);
        let appender = Appender::builder()
            .filter(filter)
            .build(APPENDER_NAME, Box::new(mock));
        let root = Root::builder()
            .appender(APPENDER_NAME)
            .build(LevelFilter::Trace);
        let config = Config::builder().appender(appender).build(root).unwrap();
        (logging_test_setup(config), logs)
    }

    fn test_setup_dynamic_level(
        name: String,
        level: LevelFilter,
    ) -> (MutexGuard<'static, ()>, LogsHandle) {
        let filter = Box::new(DynamicLevelFilter::new(name, level));
        test_setup(filter)
    }

    #[test]
    fn dlf_default() {
        let (_guard, logs_handle) =
            test_setup_dynamic_level("dlf_default".to_string(), LevelFilter::Debug);

        trace!("apple");
        debug!("banana");
        info!("cantaloupe");
        warn!("durian");
        error!("elderberry");

        let logs = logs_handle.lock().unwrap();
        assert_eq!(logs.len(), 4);
        for (line, level) in logs
            .iter()
            .zip(vec!["DEBUG", "INFO", "WARN", "ERROR"].iter())
        {
            assert!(line.contains(level));
        }
    }

    #[test]
    fn dlf_change() {
        let (_guard, logs_handle) =
            test_setup_dynamic_level("dlf_change".to_string(), LevelFilter::Error);

        info!("Hidden!");
        error!("Seen!");
        DynamicLevelFilter::set("dlf_change", LevelFilter::Info);
        info!("Seen!");
        error!("Seen!");

        let logs = logs_handle.lock().unwrap();
        assert_eq!(logs.len(), 3);
        for line in logs.iter() {
            assert!(line.contains("Seen!"));
        }
    }
}

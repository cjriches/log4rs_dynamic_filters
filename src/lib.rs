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

/// Add this module's deserializers to the given `Deserializers`.
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
#[derive(Debug)]
pub struct DynamicLevelFilter {
    name: String,
}

impl DynamicLevelFilter {
    /// Create a `DynamicLevelFilter` with the given name. If that name is unused, register it and
    /// set its level to the given `LevelFilter`.
    pub fn new(name: String, starting_level: LevelFilter) -> Self {
        let mut filters = DYNAMIC_LEVEL_FILTERS.write().unwrap();
        if !filters.contains_key(&name) {
            let result = filters.insert(name.clone(), starting_level);
            debug_assert!(result.is_none());
        }

        DynamicLevelFilter { name }
    }

    /// Set the DynamicLevelFilter with the given name to the given level.
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

/// Configure a `DynamicLevelFilter` from a config file.
#[derive(Debug, serde::Deserialize)]
struct DynamicLevelFilterConfig {
    name: String,
    default: LevelFilter,
}

/// Deserialize a `DynamicLevelFilterConfig` into a `DynamicLevelFilter`.
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
    use log4rs_test_utils::{logging_test_setup, LogsHandle, MockAppender};
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

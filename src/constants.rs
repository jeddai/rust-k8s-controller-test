use crate::{Error::InvalidConfigurationError, Result};
use std::collections::HashMap;
use std::env;

pub static FINALIZER: &str = "nimbus.mozilla.org/finalizer";

pub static ENABLED: &str = "nimbus=enabled";
pub static ANNOTATION_PREFIX: &str = "nimbus.mozilla.org/";

pub static REMOTE_SETTING_URL: &str = "REMOTE_SETTING_URL";
pub static REMOTE_SETTING_REFRESH_RATE_IN_SECONDS: &str = "REMOTE_SETTING_REFRESH_RATE_IN_SECONDS";
pub static APP_ID: &str = "APP_ID";
pub static APP_NAME: &str = "APP_NAME";
pub static CHANNEL: &str = "CHANNEL";
pub static CIRRUS_FML_PATH: &str = "CIRRUS_FML_PATH";

pub struct CirrusEnvironment {
    pub map: HashMap<String, String>,
}

impl Default for CirrusEnvironment {
    fn default() -> Self {
        Self {
            map: HashMap::<String, String>::from_iter([
                (REMOTE_SETTING_URL.into(), env::var(REMOTE_SETTING_URL).unwrap_or("https://firefox.settings.services.mozilla.com/v1/buckets/main/collections/nimbus-web-experiments/records".into())),
                (REMOTE_SETTING_REFRESH_RATE_IN_SECONDS.into(), env::var(REMOTE_SETTING_REFRESH_RATE_IN_SECONDS).unwrap_or("10".into())),
                (APP_ID.into(), env::var(APP_ID).unwrap_or("".into())),
                (APP_NAME.into(), env::var(APP_NAME).unwrap_or("".into())),
                (CHANNEL.into(), env::var(CHANNEL).unwrap_or("".into())),
                (CIRRUS_FML_PATH.into(), env::var(CIRRUS_FML_PATH).unwrap_or("/nimbus.fml.yaml".into())),
            ])
        }
    }
}

impl PartialEq<CirrusEnvironment> for CirrusEnvironment {
    fn eq(&self, other: &CirrusEnvironment) -> bool {
        self.map == other.map
    }
}

impl Eq for CirrusEnvironment {}

impl IntoIterator for CirrusEnvironment {
    type Item = (String, String);
    type IntoIter = std::collections::hash_map::IntoIter<String, String>;

    fn into_iter(self) -> Self::IntoIter {
        self.map.into_iter()
    }
}

impl CirrusEnvironment {
    pub fn set(&mut self, key: String, value: String) -> Option<String> {
        self.map.insert(key, value)
    }

    fn check_var_not_empty(&self, key: &'static str, name: &str) -> Result<()> {
        if self.map.get(key).unwrap().is_empty() {
            let error = format!("{} env var is empty — add a value for the 'nimbus.mozilla.org/env.{}' annotation onto deployment {}.", key, key.replace('_', ".").to_lowercase(), name);
            return Err(InvalidConfigurationError(error));
        }
        Ok(())
    }

    pub fn validate(&self, name: &str) -> Result<()> {
        self.check_var_not_empty(APP_ID, name)?;
        self.check_var_not_empty(APP_NAME, name)?;
        self.check_var_not_empty(CHANNEL, name)?;
        Ok(())
    }
}

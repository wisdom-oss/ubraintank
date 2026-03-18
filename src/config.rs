use alloc::{borrow::Cow, string::String};
use esp_nvs::{Key, Nvs, platform::Platform};

use crate::nvs::Keys;

#[derive(Debug)]
pub struct Config {
    pub device: DeviceConfig,
    pub wifi: WifiConfig,
    pub api: ApiConfig,
}

#[derive(Debug)]
pub struct DeviceConfig {
    pub name: String,
}

#[derive(Debug)]
pub struct WifiConfig {
    pub ssid: String,
    pub pass: String,
}

#[derive(Debug)]
pub struct ApiConfig {
    pub report_url: String,
}

#[derive(Debug)]
pub struct Error {
    pub namespace: Cow<'static, Key>,
    pub key: Cow<'static, Key>,
    pub error: esp_nvs::error::Error,
}

impl Config {
    pub fn try_from_nvs<T: Platform>(nvs: &mut Nvs<T>) -> Result<Self, Error> {
        let mut get = |namespace, key| {
            nvs.get(namespace, key).map_err(|error| Error {
                namespace: Cow::Borrowed(namespace),
                key: Cow::Borrowed(key),
                error,
            })
        };

        Ok(Config {
            device: DeviceConfig {
                name: get(Keys::DEVICE, Keys::NAME)?,
            },
            wifi: WifiConfig {
                ssid: get(Keys::WIFI, Keys::SSID)?,
                pass: get(Keys::WIFI, Keys::PASS)?,
            },
            api: ApiConfig {
                report_url: get(Keys::API, Keys::REPORT_URL)?,
            },
        })
    }
}

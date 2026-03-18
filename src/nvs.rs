use esp_nvs::{Key, Nvs, platform::Platform};

const PARTITION_OFFSET: usize = crate::DEVICE.partition_data.nvs.offset as usize;
const PARTITION_SIZE: usize = crate::DEVICE.partition_data.nvs.size as usize;

pub fn new<T: Platform>(hal: T) -> Result<Nvs<T>, esp_nvs::error::Error> {
    Nvs::new(PARTITION_OFFSET, PARTITION_SIZE, hal)
}

pub struct Keys;
impl Keys {
    pub const DEVICE: &Key = &Key::from_str("device");
    pub const NAME: &Key = &Key::from_str("name");

    pub const WIFI: &Key = &Key::from_str("wifi");
    pub const SSID: &Key = &Key::from_str("ssid");
    pub const PASS: &Key = &Key::from_str("pass");

    pub const API: &Key = &Key::from_str("api");
    pub const REPORT_URL: &Key = &Key::from_str("report_url");
}
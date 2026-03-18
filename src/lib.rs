#![no_std]

extern crate alloc;

static_toml::static_toml! {
    const DEVICE = include_toml!("device.toml");
}

pub mod config;
pub mod nvs;

#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use alloc::{boxed::Box, format};
use embassy_executor::Spawner;
use embassy_net::{DhcpConfig, Runner, StackResources};
use embassy_time::{Duration, Timer};
use esp_hal::clock::CpuClock;
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::timer::timg::TimerGroup;
use esp_radio::wifi::{ClientConfig, ModeConfig, WifiDevice};
use esp_storage::FlashStorage;
use log::*;
use ubraintank::config::Config;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    // generator version: 1.2.0

    esp_println::logger::init_logger_from_env();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 98768);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0);

    info!("Embassy initialized!");

    let storage = FlashStorage::new(peripherals.FLASH);
    let mut nvs = ubraintank::nvs::new(storage).expect("Failed to create NVS storage");

    let config = Config::try_from_nvs(&mut nvs).unwrap_or_else(|err| {
        error!(
            "Could not read config from NVS, at namespace {:?} for key {:?}",
            err.namespace, err.key
        );

        panic!();
    });

    info!("{config:#?}");

    let radio_init = esp_radio::init().expect("Failed to initialize Wi-Fi/BLE controller");
    let radio_init = Box::leak(Box::new(radio_init));
    let (mut wifi_controller, interfaces) =
        esp_radio::wifi::new(radio_init, peripherals.WIFI, Default::default())
            .expect("Failed to initialize Wi-Fi controller");

    wifi_controller
        .set_config(&ModeConfig::Client(
            ClientConfig::default()
                .with_ssid(config.wifi.ssid)
                .with_password(config.wifi.pass),
        ))
        .expect("Failed to configure Wi-Fi");

    wifi_controller
        .start_async()
        .await
        .expect("Failed to start Wi-Fi");
    wifi_controller
        .connect_async()
        .await
        .expect("Failed to connect to AP");

    let resources = Box::leak(Box::new(StackResources::<4>::new()));
    let mut dhcp_config = DhcpConfig::default();
    let mut hostname = format!("ubraintank-{}", config.device.name);
    hostname.truncate(32); // max dhcp hostname length
    dhcp_config.hostname = Some(hostname.parse().expect("truncated to 32 chars"));
    let (wifi_stack, wifi_runner) = embassy_net::new(
        interfaces.sta,
        embassy_net::Config::dhcpv4(dhcp_config),
        resources,
        123,
    );

    spawner
        .spawn(run_net(wifi_runner))
        .expect("Failed to spawn runner task");

    wifi_stack.wait_config_up().await;
    info!("{:#?}", wifi_stack.config_v4());

    let mut led = Output::new(peripherals.GPIO2, Level::High, OutputConfig::default());

    loop {
        led.toggle();
        Timer::after(Duration::from_secs(1)).await;
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0/examples
}

#[embassy_executor::task]
async fn run_net(mut runner: Runner<'static, WifiDevice<'static>>) {
    runner.run().await;
}

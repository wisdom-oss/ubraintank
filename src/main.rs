#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use core::cmp;
use core::ffi::CStr;

use alloc::{boxed::Box, format};
use embassy_executor::Spawner;
use embassy_net::dns::DnsSocket;
use embassy_net::tcp::client::{TcpClient, TcpClientState};
use embassy_net::{DhcpConfig, Runner, StackResources};
use embassy_time::{Duration, Timer};
use esp_hal::analog::adc::{Adc, AdcConfig, Attenuation};
use esp_hal::clock::CpuClock;
use esp_hal::delay::Delay;
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::peripherals::GPIO2;
use esp_hal::timer::timg::TimerGroup;
use esp_radio::wifi::{ClientConfig, ModeConfig, WifiDevice};
use esp_storage::FlashStorage;
use log::*;
use reqwless::client::HttpClient;
use reqwless::request::Method;
use ubraintank::config::Config;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    let delay = Delay::new();
    if let Some(builtin_led_pin) = BUILTIN_LED_PIN.poll() {
        let mut builtin_led_pin = builtin_led_pin.lock();
        let builtin_led_pin = builtin_led_pin.reborrow();
        let mut builtin_led = Output::new(builtin_led_pin, Level::Low, OutputConfig::default());

        loop {
            delay.delay_millis(600);
            builtin_led.set_high();
            delay.delay_millis(100);
            builtin_led.set_low();
            delay.delay_millis(100);
            builtin_led.set_high();
            delay.delay_millis(100);
            builtin_led.set_low();

            error!("help");
        }
    }

    loop {}
}

extern crate alloc;

static BUILTIN_LED_PIN: spin::Once<spin::Mutex<GPIO2>> = spin::Once::new();

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see:
// <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    esp_println::logger::init_logger_from_env();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);
    BUILTIN_LED_PIN.call_once(|| spin::Mutex::new(peripherals.GPIO2));

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

    const N: usize = 1;
    const TX_SZ: usize = 2048; // 2 kB
    const RX_SZ: usize = 2048;
    let tcp_client_state = TcpClientState::<N, TX_SZ, RX_SZ>::new();
    let tcp_client = TcpClient::new(wifi_stack, &tcp_client_state);
    let dns = DnsSocket::new(wifi_stack);
    let mut http_client = HttpClient::new(&tcp_client, &dns);

    let mut builtin_led_pin = BUILTIN_LED_PIN.wait().lock();
    let builtin_led_pin = builtin_led_pin.reborrow();
    let mut builtin_led_pin = Output::new(builtin_led_pin, Level::High, OutputConfig::default());

    let mut relais = Output::new(peripherals.GPIO25, Level::Low, OutputConfig::default());

    let mut adc_config = AdcConfig::new();
    let mut sensor = adc_config.enable_pin(peripherals.GPIO35, Attenuation::_11dB);
    let mut adc = Adc::new(peripherals.ADC1, adc_config);

    let mut max_reading: u16 = 1;

    loop {
        builtin_led_pin.toggle();
        relais.toggle();

        match nb::block!(adc.read_oneshot(&mut sensor)) {
            Err(_) => error!("could not read sensor"),
            Ok(reading) => {
                max_reading = cmp::max(reading, max_reading);
                let percent = ((reading as f64 / max_reading as f64) * 100.0) as u8;
                info!("sensor reading: {reading} ({percent}%)");
            }
        }

        let url = format!("{}/ping", config.api.report_url);
        let mut rx_buf = [0; 4 * 1024];
        http_client
            .request(Method::GET, &url)
            .await
            .unwrap()
            .send(&mut rx_buf)
            .await
            .unwrap();
        let body_str = CStr::from_bytes_until_nul(&rx_buf).unwrap();
        let body_str = body_str.to_str().unwrap();
        info!("GOT RESPONSE");
        body_str
            .lines()
            .filter(|line| !line.is_empty())
            .for_each(|line| info!("{line}"));

        Timer::after(Duration::from_secs(1)).await;
    }
}

#[embassy_executor::task]
async fn run_net(mut runner: Runner<'static, WifiDevice<'static>>) {
    runner.run().await;
}

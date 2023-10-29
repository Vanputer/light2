// sudo adduser <user> dialout
// sudo chmod a+rw /dev/ttyACM0
//
// https://medium.com/@rajeshpachaikani/connect-esp32-to-wifi-with-rust-7d12532f539b
// https://github.com/esp-rs/std-training/blob/main/intro/http-server/examples/http_server.rs
// https://esp-rs.github.io/book/
// https://github.com/esp-rs/esp-idf-hal/blob/master/src/ledc.rs

use embedded_svc::wifi::{ClientConfiguration, Configuration, Wifi};
use embedded_svc::{
    http::server::{HandlerError, Request},
    http::Method,
    io::Write,
};
use esp_idf_hal::delay::Delay;
use esp_idf_hal::{gpio::*, peripherals::Peripherals};
use esp_idf_hal::adc::config::Config as adc_Config;
use esp_idf_hal::adc::*;
// use esp_idf_hal::adc::attenuation;
//use esp_idf_hal::adc::{AdcChannelDriver, AdcDriver, Attenuation};
use esp_idf_hal::ledc::{config::TimerConfig, LedcDriver, LedcTimerDriver};
use esp_idf_hal::units::*;
use esp_idf_svc::http::server::Configuration as SVC_Configuration;
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    http::server::{EspHttpConnection, EspHttpServer},
    nvs::EspDefaultNvsPartition,
    wifi::EspWifi,
};
use esp_idf_sys as _;
use querystring;
use serde_json;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    thread,
    thread::sleep,
    time::Duration,
};

use device::{Action, Device};

fn main() {
    esp_idf_sys::link_patches(); //Needed for esp32-rs
    println!("Entered Main function!");
    let peripherals = Peripherals::take().unwrap();
    let sys_loop = EspSystemEventLoop::take().unwrap();
    let nvs = EspDefaultNvsPartition::take().unwrap();

    let mut wifi_driver = EspWifi::new(peripherals.modem, sys_loop, Some(nvs)).unwrap();

    wifi_driver
        .set_configuration(&Configuration::Client(ClientConfiguration {
            ssid: "ssid".into(),
            password: "password".into(),
            ..Default::default()
        }))
        .unwrap();

    wifi_driver.start().unwrap();
    wifi_driver.connect().unwrap();
    while !wifi_driver.is_connected().unwrap() {
        let config = wifi_driver.get_configuration().unwrap();
        println!("Waiting for station {:?}", config);
    }
    println!("Should be connected now");

    let light_1 = Arc::new(Mutex::new(Device {
        name: "roof vent".to_string(),
        action: Action::Off,
        available_actions: Vec::from([
            Action::On,
            Action::Off,
            Action::Up,
            Action::Down,
            Action::Set,
        ]),
        default_target: 3,
        duty_cycles: [0, 20, 40, 60, 80, 96],
        target: 0,
        freq_kHz: 1,
    }));

    // let freq_khz = { (1. /light_1.lock().unwrap().period_ms as f64 * 1000.).round() as usize };
    let driver_1 = Arc::new(Mutex::new(LedcDriver::new(
        peripherals.ledc.channel0,
        LedcTimerDriver::new(
            peripherals.ledc.timer0,
            &TimerConfig::new().frequency(25.kHz().into()),
        ).unwrap(),
        peripherals.pins.gpio4,
    ).unwrap()));

    let mut adc = AdcDriver::new(peripherals.adc1, &adc_Config::new().calibration(true)).unwrap();
    // Configuring pin to analogy read, you can regulate the adc input voltage range depending
    // on your need
    // for this example we use the attenuation of 11db which sets the niput voltage range to
    // around 0-3.6V
    // https://github.com/esp-rs/esp-idf-hal/blob/master/examples/adc.rs
    // https://apollolabsblog.hashnode.dev/esp32-standard-library-embedded-rust-analog-temperature-sensing-using-the-adc
    let mut adc_pin_1: esp_idf_hal::adc::AdcChannelDriver<'_, Gpio5, Atten11dB<_>> = 
        AdcChannelDriver::new(peripherals.pins.gpio5).unwrap();
    let mut adc_pin_1 = Arc::new(Mutex::new(adc_pin_1));

    // roof_vent Manager
    let light_1_clone = light_1.clone();
    let driver_1_clone = driver_1.clone();
    let adc_pin_1_clone = adc_pin_1.clone();
    thread::spawn(move || {
        let max_duty = { driver_1_clone.lock().unwrap().get_max_duty() };
        let voltages = [680, 1360, 2040, 2720, 3400, 50000];
        loop {
            {
                let mut light = light_1_clone.lock().unwrap();
                // probably just something about getting the voltage of the pin and then
                // getting the segment and then using Set to set it
                let mut adc_pin_1 = adc_pin_1_clone.lock().unwrap();
                let voltage = adc.read(&mut adc_pin_1).unwrap();
                let mut i = 0;
                for v in voltages {
                    if voltage > v {
                        i += 1;
                    } else if voltage < v {
                        break;
                    }
                }
                light.take_action(Action::Set, Some(i));
                let duty_cycle = light.get_duty_cycle();
                let duty_cycle = duty_cycle * max_duty;
                driver_1_clone.lock().unwrap().set_duty(duty_cycle);
            }
            Delay::delay_ms(100);
        }
    });

    loop {
        println!(
            "IP info: {:?}",
            wifi_driver.sta_netif().get_ip_info().unwrap()
        );
        sleep(Duration::new(10, 0));
    }
}

fn roof_vent_thread_spawner(vent: Arc<Mutex<Device>>, louver: Arc<Mutex<Device>>, pin1: Gpio1) {

}

fn exit_early<'a>(
    request: Request<&mut EspHttpConnection<'a>>,
    message: &str,
    code: u16,
) -> Result<(), HandlerError> {
    let mut response = request.into_status_response(422)?;
    response.write_all(message.as_bytes());
    Ok(())
}

fn index_html() -> String {
    templated("Hello from ESP32-C3!")
}

fn templated(content: impl AsRef<str>) -> String {
    format!(
        r#"
<!DOCTYPE html>
<html>
    <head>
        <meta charset="utf-8">
        <title>esp-rs web server</title>
    </head>
    <body>
        {}
    </body>
</html>
"#,
        content.as_ref()
    )
}


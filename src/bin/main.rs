#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

mod protocol;

use esp_hal::clock::CpuClock;
use esp_hal::delay::Delay;
use esp_hal::gpio::Flex;
use esp_hal::main;
use esp_println::println;

use protocol::sensor::Dht22;

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    // Print the panic info if you have esp-println set up
    println!("{}", _info);
    loop {}
}

// This creates a default app-descriptor required by the esp-idf bootloader.
esp_bootloader_esp_idf::esp_app_desc!();

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[main]
fn main() -> ! {
    // generator version: 1.3.0
    // generator parameters: --chip esp32 -o esp32-wroom-32

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    let delay = Delay::new();

    // DHT22 data line on D4 (GPIO4). The driver configures it as open-drain
    // with the internal pull-up; an external 4.7k-10k pull-up is still recommended.
    let mut data_pin = Flex::new(peripherals.GPIO4);
    let mut sensor = Dht22::new(&mut data_pin, delay);

    // The DHT22 needs ~1-2 s after power-up before the first read.
    delay.delay_millis(2000);

    loop {
        match sensor.read() {
            Ok(reading) => {
                let t = reading.temperature;
                let h = reading.humidity;
                println!(
                    "Temperature: {}{}.{} C  Humidity: {}.{} %",
                    if t < 0 { "-" } else { "" },
                    t.abs() / 10,
                    t.abs() % 10,
                    h / 10,
                    h % 10
                );
            }
            Err(e) => println!("DHT22 read failed: {:?}", e),
        }

        // DHT22 minimum sampling period is 2 s.
        delay.delay_millis(2000);
    }
}

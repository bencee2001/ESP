#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

extern crate alloc;

mod config;
mod protocol;
mod secrets;

use core::fmt::Write as _;

use embassy_executor::Spawner;
use embassy_net::{IpAddress, IpEndpoint, Runner, Stack, StackResources, tcp::TcpSocket};
use embassy_time::{Duration, Timer};
use embedded_io_async::Write as _;
use esp_hal::clock::CpuClock;
use esp_hal::delay::Delay;
use esp_hal::gpio::Flex;
use esp_hal::rng::Rng;
use esp_hal::timer::timg::TimerGroup;
use esp_println::println;
use esp_radio::wifi::{Config as WifiConfig, Interface, WifiController, sta::StationConfig};
use static_cell::StaticCell;

use protocol::sensor::{Dht22, DhtSensorReading};

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("{}", info);
    loop {}
}

// This creates a default app-descriptor required by the esp-idf bootloader.
esp_bootloader_esp_idf::esp_app_desc!();

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 98768);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_interrupt =
        esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_interrupt.software_interrupt0);

    // --- WiFi + network stack ---
    let (wifi_controller, interfaces) =
        esp_radio::wifi::new(peripherals.WIFI, Default::default())
            .expect("Failed to initialize Wi-Fi controller");

    let rng = Rng::new();
    let seed = ((rng.random() as u64) << 32) | rng.random() as u64;

    static RESOURCES: StaticCell<StackResources<3>> = StaticCell::new();
    let (stack, runner) = embassy_net::new(
        interfaces.station,
        embassy_net::Config::dhcpv4(Default::default()),
        RESOURCES.init(StackResources::new()),
        seed,
    );

    spawner.spawn(wifi_connection(wifi_controller).unwrap());
    spawner.spawn(net_task(runner).unwrap());

    // --- Sensor ---
    static DATA_PIN: StaticCell<Flex<'static>> = StaticCell::new();
    let data_pin = DATA_PIN.init(Flex::new(peripherals.GPIO4));
    let sensor = Dht22::new(data_pin, Delay::new());

    spawner.spawn(sensor_task(stack, sensor).unwrap());

    loop {
        stack.wait_config_up().await;
        if let Some(cfg) = stack.config_v4() {
            println!("Got IP: {}", cfg.address);
        }
        stack.wait_config_down().await;
        println!("Lost IP");
    }
}

/// Keeps the station connected to the access point, reconnecting when dropped.
#[embassy_executor::task]
async fn wifi_connection(mut controller: WifiController<'static>) {
    let station = WifiConfig::Station(
        StationConfig::default()
            .with_ssid(secrets::WIFI_SSID)
            .with_password(secrets::WIFI_PASSWORD.into()),
    );
    controller
        .set_config(&station)
        .expect("Failed to apply WiFi station config");

    loop {
        println!("Connecting to WiFi '{}'...", secrets::WIFI_SSID);
        match controller.connect_async().await {
            Ok(_) => {
                println!("WiFi connected");
                // Returns when the link drops; then loop around and reconnect.
                let _ = controller.wait_for_disconnect_async().await;
                println!("WiFi disconnected");
            }
            Err(e) => {
                println!("WiFi connect failed: {:?}", e);
                Timer::after(Duration::from_secs(5)).await;
            }
        }
    }
}

/// Drives the TCP/IP stack.
#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, Interface<'static>>) -> ! {
    runner.run().await
}

/// Reads the DHT22 and POSTs each reading to the server.
#[embassy_executor::task]
async fn sensor_task(stack: Stack<'static>, mut sensor: Dht22<'static, 'static, Delay>) {
    let mut rx_buf = [0u8; 1024];
    let mut tx_buf = [0u8; 1024];

    loop {
        match read_with_retry(&mut sensor) {
            Some(reading) => {
                println!(
                    "Temperature: {}.{} C  Humidity: {}.{} %",
                    reading.temperature / 10,
                    (reading.temperature % 10).abs(),
                    reading.humidity / 10,
                    reading.humidity % 10
                );
                if !stack.is_config_up() {
                    println!("No network yet, skipping upload");
                } else if let Err(e) = post_reading(stack, &reading, &mut rx_buf, &mut tx_buf).await {
                    println!("Upload failed: {}", e);
                }
            }
            None => println!("DHT22 read failed after retries"),
        }

        Timer::after(Duration::from_secs(config::SEND_INTERVAL_SECS)).await;
    }
}

/// WiFi interrupts can disturb the bit-banged DHT22 timing, so retry a few times.
fn read_with_retry(sensor: &mut Dht22<'static, 'static, Delay>) -> Option<DhtSensorReading> {
    for attempt in 1..=3 {
        match sensor.read() {
            Ok(r) => return Some(r),
            Err(e) => {
                println!("DHT22 read attempt {} failed: {:?}", attempt, e);
                Delay::new().delay_millis(2500);
            }
        }
    }
    None
}

async fn post_reading(
    stack: Stack<'static>,
    reading: &DhtSensorReading,
    rx_buf: &mut [u8],
    tx_buf: &mut [u8],
) -> Result<(), &'static str> {
    // Body: {"s_id":1,"t":21.2,"h":42.8}
    let mut body: heapless::String<128> = heapless::String::new();
    write!(
        body,
        "{{\"s_id\":{},\"t\":{}.{},\"h\":{}.{}}}",
        config::SENSOR_ID,
        reading.temperature / 10,
        (reading.temperature % 10).abs(),
        reading.humidity / 10,
        reading.humidity % 10
    )
    .map_err(|_| "body too long")?;

    let mut request: heapless::String<384> = heapless::String::new();
    let ip = secrets::SERVER_IP;
    write!(
        request,
        "POST {} HTTP/1.1\r\nHost: {}.{}.{}.{}:{}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        config::SERVER_ENDPOINT,
        ip[0], ip[1], ip[2], ip[3], config::SERVER_PORT,
        body.len(),
        body
    )
    .map_err(|_| "request too long")?;

    let mut socket = TcpSocket::new(stack, rx_buf, tx_buf);
    socket.set_timeout(Some(Duration::from_secs(10)));

    let endpoint = IpEndpoint::new(IpAddress::v4(ip[0], ip[1], ip[2], ip[3]), config::SERVER_PORT);
    socket.connect(endpoint).await.map_err(|_| "connect failed")?;
    socket
        .write_all(request.as_bytes())
        .await
        .map_err(|_| "write failed")?;
    socket.flush().await.map_err(|_| "flush failed")?;

    // Read the status line so we know the server accepted it. Any 2xx is success.
    let mut resp = [0u8; 256];
    let n = socket.read(&mut resp).await.map_err(|_| "read failed")?;
    let status_line = core::str::from_utf8(&resp[..n])
        .unwrap_or("")
        .lines()
        .next()
        .unwrap_or("");
    let ok = status_line
        .split(' ')
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .is_some_and(|code| (200..300).contains(&code));
    socket.close();

    if ok {
        println!("Sent {} -> {}", body, status_line);
        Ok(())
    } else {
        println!("Server replied: {}", status_line);
        Err("server rejected the reading")
    }
}

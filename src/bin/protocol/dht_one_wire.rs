use embedded_hal::{delay::DelayNs, digital::PinState};
use esp_hal::gpio::{DriveMode, Flex, InputConfig, OutputConfig, Pull};
use esp_hal::time::{Duration, Instant};

use crate::protocol::error::WaitError;

pub struct DhtOneWire<'a, 'd, D: DelayNs> {
    pin: &'a mut Flex<'d>,
    delay: D,
}

impl<'a, 'd, D: DelayNs> DhtOneWire<'a, 'd, D> {
    /// Configures the pin as an open-drain output with the internal pull-up and
    /// the input stage enabled, so the same pin can both drive the start pulse
    /// low and read the sensor's response.
    pub fn new(pin: &'a mut Flex<'d>, delay: D) -> Self {
        pin.apply_output_config(
            &OutputConfig::default()
                .with_drive_mode(DriveMode::OpenDrain)
                .with_pull(Pull::Up),
        );
        pin.apply_input_config(&InputConfig::default().with_pull(Pull::Up));
        pin.set_high(); // release the line (pulled up)
        pin.set_output_enable(true);
        pin.set_input_enable(true);
        Self { pin, delay }
    }

    pub fn read_dht_protocol(&mut self) -> Result<[u8; 5], WaitError> {
        self.init_sensor_read()?;
        self.wait_sensor_ack()?;
        self.read_data()
    }

    /// Busy-waits until the pin reaches `goal_pin_state`, or `timeout_us`
    /// microseconds elapse. Returns how long the wait took in microseconds.
    fn wait_for_state(
        &mut self,
        goal_pin_state: PinState,
        timeout_us: u64,
        error: WaitError,
    ) -> Result<u64, WaitError> {
        let goal_state_bool = bool::from(goal_pin_state); // High = true
        let start = Instant::now();
        let timeout = Duration::from_micros(timeout_us);

        loop {
            if self.pin.is_high() == goal_state_bool {
                return Ok(start.elapsed().as_micros());
            }
            if start.elapsed() > timeout {
                return Err(error);
            }
        }
    }

    fn init_sensor_read(&mut self) -> Result<(), WaitError> {
        // Host start signal: pull low for >= 1 ms (DHT22) / >= 18 ms (DHT11).
        self.pin.set_low();
        self.delay.delay_ms(20);
        // Release the line; the pull-up brings it high and the sensor responds.
        self.pin.set_high();
        self.wait_for_state(PinState::Low, 40, WaitError::TimeoutInit)?;
        Ok(())
    }

    fn wait_sensor_ack(&mut self) -> Result<(), WaitError> {
        self.wait_for_state(PinState::High, 80, WaitError::TimeoutAck)?;
        self.wait_for_state(PinState::Low, 80, WaitError::TimeoutAck)?;
        Ok(())
    }

    fn read_data(&mut self) -> Result<[u8; 5], WaitError> {
        let mut data = [0u8; 5];
        for i in 0..40 {
            let bit = self.read_bit()?;

            let byte_index = i / 8;
            data[byte_index] <<= 1;

            if bit {
                data[byte_index] |= 1;
            }
        }

        let checksum = data[0]
            .wrapping_add(data[1])
            .wrapping_add(data[2])
            .wrapping_add(data[3]);

        if checksum == data[4] {
            Ok(data)
        } else {
            Err(WaitError::CheckSum(data))
        }
    }

    fn read_bit(&mut self) -> Result<bool, WaitError> {
        // Each bit: ~50 us low, then high for ~26-28 us (0) or ~70 us (1).
        // Measure the high pulse width instead of sampling at a fixed offset,
        // which is robust against polling overhead.
        self.wait_for_state(PinState::High, 100, WaitError::TimeoutRead)?;
        let high_us = self.wait_for_state(PinState::Low, 150, WaitError::TimeoutRead)?;
        Ok(high_us > 45)
    }
}

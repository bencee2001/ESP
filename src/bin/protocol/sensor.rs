use embedded_hal::delay::DelayNs;

use crate::protocol::{dht_one_wire::DhtOneWire, error::WaitError};
use esp_hal::gpio::Flex;

pub struct DhtSensorReading {
    pub temperature: i16,
    pub humidity: u16,
}

pub struct Dht11<'a, 'd, D: DelayNs> {
    protocol: DhtOneWire<'a, 'd, D>,
}

impl<'a, 'd, D: DelayNs> Dht11<'a, 'd, D> {
    pub fn new(pin: &'a mut Flex<'d>, delay: D) -> Self {
        Self {
            protocol: DhtOneWire::new(pin, delay),
        }
    }

    pub fn read(&mut self) -> Result<DhtSensorReading, WaitError> {
        let data = self.protocol.read_dht_protocol()?;

        Ok(DhtSensorReading {
            temperature: (data[2] as i16) * 10,
            humidity: (data[0] as u16) * 10,
        })
    }
}

pub struct Dht22<'a, 'd, D: DelayNs> {
    protocol: DhtOneWire<'a, 'd, D>,
}

impl<'a, 'd, D: DelayNs> Dht22<'a, 'd, D> {
    pub fn new(pin: &'a mut Flex<'d>, delay: D) -> Self {
        Self {
            protocol: DhtOneWire::new(pin, delay),
        }
    }

    pub fn read(&mut self) -> Result<DhtSensorReading, WaitError> {
        let data = self.protocol.read_dht_protocol()?;

        let raw_humidity = ((data[0] as u16) << 8) | (data[1] as u16);

        let raw_temp = (((data[2] & 0x7F) as u16) << 8) | (data[3] as u16);
        let mut temperature = raw_temp as i16;

        if (data[2] & 0x80) != 0 {
            temperature = -temperature;
        }

        Ok(DhtSensorReading {
            temperature,
            humidity: raw_humidity,
        })
    }
}

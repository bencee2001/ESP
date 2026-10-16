#[derive(ufmt::derive::uDebug, Debug)]
pub enum WaitError {
    Hardware,
    CheckSum([u8; 5]),
    Timeout,
    HardwareInit,
    TimeoutInit,
    HardwareAck,
    TimeoutAck,
    HardwareRead,
    TimeoutRead,
}

impl ufmt::uDisplay for WaitError {
    fn fmt<W: ufmt::uWrite + ?Sized>(
        &self,
        f: &mut ufmt::Formatter<'_, W>,
    ) -> Result<(), W::Error> {
        match self {
            WaitError::Timeout => f.write_str("Timeout"),
            WaitError::Hardware => f.write_str("Hardware Error"),
            WaitError::CheckSum(d) => ufmt::uwrite!(
                f,
                "CheckSum Error (raw: {} {} {} {} {})",
                d[0],
                d[1],
                d[2],
                d[3],
                d[4]
            ),
            WaitError::HardwareInit => f.write_str("Hardware Init Error"),
            WaitError::TimeoutInit => f.write_str("Timeout Init Error"),
            WaitError::HardwareAck => f.write_str("Hardware Ack Error"),
            WaitError::TimeoutAck => f.write_str("Timeout Ack Error"),
            WaitError::HardwareRead => f.write_str("Hardware Read Error"),
            WaitError::TimeoutRead => f.write_str("Timeout Read Error"),
        }
    }
}

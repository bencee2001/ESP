//! Copy this file to `src/bin/secrets.rs` and fill in the real password.
//! `secrets.rs` is listed in .gitignore and must never be committed.

pub const WIFI_PASSWORD: &str = "CHANGE_ME";

/// WiFi network name (SSID).
pub const WIFI_SSID: &str = "CHANGE_ME";

/// IPv4 address of the server that receives the readings.
pub const SERVER_IP: [u8; 4] = [0, 0, 0, 0];

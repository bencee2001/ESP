//! Non-secret settings. Edit these to point at your network and server.
//! The WiFi password lives in `secrets.rs`, which is not committed to git.

/// Identifier sent with every reading so the server can tell sensors apart.
pub const SENSOR_ID: u32 = 1;

/// TCP port the server listens on.
pub const SERVER_PORT: u16 = 8080;

/// HTTP path the readings are POSTed to.
pub const SERVER_ENDPOINT: &str = "/readings";

/// Seconds between two readings / uploads. The DHT22 needs at least 2 s.
pub const SEND_INTERVAL_SECS: u64 = 10;

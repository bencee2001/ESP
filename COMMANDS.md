# Commands cheat sheet

Everything needed to build, flash and debug this ESP32 DHT22 → HTTP firmware
from WSL2 (Ubuntu) on a Windows host.

Current setup (checked 2026-10-25):

| Item | Value |
|---|---|
| Chip | ESP32 (Xtensa), target `xtensa-esp32-none-elf` |
| Toolchain | `esp` channel via `espup` (pinned in `rust-toolchain.toml`) |
| Flasher | `espflash` 4.x (`cargo run` calls it, see `.cargo/config.toml`) |
| USB → WSL | `usbipd-win` (installed on Windows) |
| WSL networking | `mirrored` mode (`C:\Users\<you>\.wslconfig`) |
| Sensor pin | GPIO4 (DHT22 data line, needs pull-up) |
| Server target | `secrets::SERVER_IP` : `config::SERVER_PORT` (8080) `config::SERVER_ENDPOINT` (`/readings`) |

---

## 1. One-time setup

### 1.1 Rust ESP toolchain (WSL)

```bash
cargo install espup espflash
espup install                      # installs the `esp` toolchain + ~/export-esp.sh
echo '. ~/export-esp.sh' >> ~/.bashrc   # already done on this machine
source ~/.bashrc
```

Check:

```bash
rustup toolchain list              # should show "esp"
espflash --version
echo $LIBCLANG_PATH                # must point into ~/.rustup/toolchains/esp/...
```

### 1.2 Serial port permissions (WSL)

```bash
sudo usermod -aG dialout $USER     # already a member on this machine
# log out / open a new terminal afterwards
```

### 1.3 usbipd-win (Windows, once)

Install from <https://github.com/dorssel/usbipd-win/releases> or:

```powershell
winget install usbipd
```

### 1.4 Secrets file

```bash
cp src/bin/secrets.example.rs src/bin/secrets.rs
# edit WIFI_SSID, WIFI_PASSWORD and SERVER_IP in src/bin/secrets.rs
```

`secrets.rs` is git-ignored. Non-secret settings (sensor id, port, endpoint,
interval) are in `src/bin/config.rs`.

---

## 2. Pass the ESP32 USB port from Windows to WSL

Plug the board in, then in an **elevated (Administrator) PowerShell**:

```powershell
usbipd list                              # find the BUSID of the CP210x / CH340 / "USB Serial" device
usbipd bind --busid <BUSID>              # once per device, needs admin
```

Then (normal PowerShell is enough from here on, admin not required):

```powershell
usbipd attach --wsl --busid <BUSID>
```

Attach and keep re-attaching automatically after every unplug/replug:

```powershell
usbipd attach --wsl --busid <BUSID> --auto-attach
```

Give it back to Windows:

```powershell
usbipd detach --busid <BUSID>
```

Verify inside WSL:

```bash
lsusb                                    # should list the USB-UART bridge
ls /dev/ttyUSB* /dev/ttyACM*             # normally /dev/ttyUSB0
espflash board-info                      # talks to the chip, prints MAC / flash size
```

If `lsusb` shows the device but there is no `/dev/ttyUSB0`, the WSL kernel is
missing the driver. Check with `dmesg | tail` and make sure WSL is up to date
(`wsl --update` in PowerShell).

---

## 3. Build

```bash
cd ~/esp/esp_rust/esp_test

cargo build                 # debug profile (still opt-level "s")
cargo build --release       # what you normally flash
cargo clippy                # lints
cargo clean                 # if the build cache gets weird
```

Artifacts land in `target/xtensa-esp32-none-elf/{debug,release}/esp_test`.

---

## 4. Flash and monitor

`.cargo/config.toml` sets the runner to `espflash flash --monitor --chip esp32`,
so the simplest path is:

```bash
cargo run --release                      # build + flash + open serial monitor
```

Explicit variants:

```bash
cargo run --release -- --port /dev/ttyUSB0            # pick the port
espflash flash --monitor --chip esp32 target/xtensa-esp32-none-elf/release/esp_test
espflash flash --monitor --chip esp32 --port /dev/ttyUSB0 --baud 921600 \
    target/xtensa-esp32-none-elf/release/esp_test
```

Monitor only (no reflash):

```bash
espflash monitor                         # default 115200 baud
espflash monitor --port /dev/ttyUSB0
```

Exit the monitor with `Ctrl+C`. Reset the board from the monitor with `Ctrl+R`.

Other useful espflash commands:

```bash
espflash board-info
espflash erase-flash                     # full wipe
espflash save-image --chip esp32 target/xtensa-esp32-none-elf/release/esp_test firmware.bin
```

If flashing fails with "Timed out waiting for packet" or "Failed to connect",
hold **BOOT**, tap **EN/RST**, release **BOOT**, then retry.

---

## 5. Let the ESP32 reach a server running inside WSL

The firmware POSTs JSON to `SERVER_IP:8080/readings`. The ESP32 is on WiFi,
the server runs in WSL, so Windows must forward the port.

### 5.1 Current mode: `networkingMode=mirrored`

WSL already shares the Windows LAN IP. Find it:

```bash
hostname -I                               # in WSL, use the 192.168.x.x address
```

```powershell
ipconfig                                  # in Windows, same address under the WiFi/Ethernet adapter
```

Put that address in `SERVER_IP` in `src/bin/secrets.rs`.

Open the port in the Windows firewall (elevated PowerShell, once):

```powershell
New-NetFirewallRule -DisplayName "ESP readings 8080" -Direction Inbound -Protocol TCP -LocalPort 8080 -Action Allow
```

In mirrored mode the Hyper-V firewall also sits in front of WSL. If the ESP
still cannot connect, allow inbound traffic to WSL as well:

```powershell
Set-NetFirewallHyperVVMSetting -Name '{40E0AC32-E211-11EA-991E-54CBE2B36B4C}' -DefaultInboundAction Allow
```

(or a narrower rule)

```powershell
New-NetFirewallHyperVRule -Name "ESP-8080" -DisplayName "ESP readings 8080" -Direction Inbound -VMCreatorId '{40E0AC32-E211-11EA-991E-54CBE2B36B4C}' -Protocol TCP -LocalPorts 8080
```

Restart WSL after changing `.wslconfig`:

```powershell
wsl --shutdown
```

### 5.2 Fallback: default NAT mode (no `.wslconfig` or `networkingMode=nat`)

WSL gets a private 172.x.x.x address that the ESP cannot see. Forward the
Windows port to it (elevated PowerShell):

```powershell
$wslIp = (wsl -d Ubuntu hostname -I).Trim().Split(" ")[0]
netsh interface portproxy add v4tov4 listenport=8080 listenaddress=0.0.0.0 connectport=8080 connectaddress=$wslIp
New-NetFirewallRule -DisplayName "ESP readings 8080" -Direction Inbound -Protocol TCP -LocalPort 8080 -Action Allow
```

The WSL IP changes on every reboot, so re-run the first two lines after a
restart. Manage the rules:

```powershell
netsh interface portproxy show all
netsh interface portproxy delete v4tov4 listenport=8080 listenaddress=0.0.0.0
```

`SERVER_IP` must be the **Windows** LAN IP (from `ipconfig`), not the WSL one.

### 5.3 Test the path without the ESP32

Quick listener in WSL that prints whatever arrives on 8080:

```bash
nc -lk 8080
```

Then from another device on the WiFi (phone, laptop) or from Windows PowerShell:

```powershell
curl.exe -X POST http://<SERVER_IP>:8080/readings -H "Content-Type: application/json" -d '{"s_id":1,"t":21.2,"h":42.8}'
```

If the text shows up in the `nc` window, the forwarding and firewall are fine.
Note: `nc` never answers, so the firmware will log "read failed" against it.
That's expected for this test. A real server must reply with a 2xx status.

Minimal Python server that accepts the POSTs and replies 200:

```bash
python3 - <<'PY'
from http.server import BaseHTTPRequestHandler, HTTPServer
class H(BaseHTTPRequestHandler):
    def do_POST(self):
        n = int(self.headers.get("Content-Length", 0))
        print(self.path, self.rfile.read(n).decode())
        self.send_response(200); self.end_headers()
HTTPServer(("0.0.0.0", 8080), H).serve_forever()
PY
```

---

## 6. Typical session

```powershell
# Windows (PowerShell)
usbipd attach --wsl --busid <BUSID>
```

```bash
# WSL
cd ~/esp/esp_rust/esp_test
cargo run --release                 # flash + monitor
# expect: "Connecting to WiFi ...", "WiFi connected", "Got IP: ...",
#         "Temperature: 21.3 C  Humidity: 42.8 %", "Sent {...} -> HTTP/1.1 200 OK"
```

---

## 7. Troubleshooting

| Symptom | Fix |
|---|---|
| `cannot find value ... in module config` | Values moved to `secrets.rs`; copy `secrets.example.rs` and fill it in |
| `/dev/ttyUSB0: Permission denied` | `sudo usermod -aG dialout $USER`, then new shell |
| No `/dev/ttyUSB*` after `usbipd attach` | `lsusb`, `dmesg \| tail`, `wsl --update`, replug |
| `usbipd attach` says device is not shared | run `usbipd bind --busid <BUSID>` as admin first |
| `LIBCLANG_PATH` / `xtensa-esp32-elf-gcc not found` | `source ~/export-esp.sh` |
| `DHT22 read attempt N failed: TimeoutInit` | wiring / pull-up on GPIO4, sensor needs 3.3–5 V and ≥2 s between reads |
| `Upload failed: connect failed` | server not running, wrong `SERVER_IP`, or firewall/portproxy (section 5) |
| `Upload failed: read failed` | server accepted the TCP connection but never answered (e.g. `nc`) |
| `Server replied: HTTP/1.1 4xx/5xx` | endpoint/body mismatch on the server side |

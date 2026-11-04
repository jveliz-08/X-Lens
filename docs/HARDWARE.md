# ESP32-CAM firmware

Target: **ESP32-CAM with ESP-32S module and OV3660 camera sensor**. Manufacturer
is unknown; it is not identified as an AI-Thinker product. Board/sensor markings
were inspected, but photographs do not establish internal camera routing,
PSRAM health or flash capacity. No user photograph is stored in this repository.

Espressif's pinned camera driver supports OV3660. Runtime initialization reads
and logs the detected sensor PID and fails closed if it does not match the
selected expectation (OV3660 by default). Defaults: 320×240 JPEG (`FRAME_SIZE`), quality
12, two PSRAM framebuffers (grab latest), capture/upload/return continuously over
one keep-alive HTTPS connection, capped at ~15 fps (`MIN_FRAME_MS`). Wi-Fi power
save is off and TX power is capped at 15 dBm (`MAX_TX_POWER`). Every 5 s the log
reports fps, average JPEG size, last status, RSSI and internal heap.

Edge features (all in `main.c`):

- **Motion score**: each JPEG is decoded at 1/8 scale with `esp_jpeg` (40×30 RGB
  cells, ~28 ms at 160 MHz), converted to luma and compared with an exponential
  moving-average background. The mean shift is subtracted first, so auto-exposure
  or a light switch does not count as motion. The changed-cell ratio (per mille)
  is sent as `X-Motion`. Motion is not reported for 4 s after the sensor starts.
- **Ignore zones**: every upload reply carries `X-Motion-Mask` (12 hex digits, an
  8×6 grid). Cells in the mask still update the background but are excluded from
  the score, which becomes changed/watched cells. The mask arrives with the next
  upload after the operator saves it; no reboot, no NVS write.
- **Demand-driven rate**: full rate while motion was seen in the last 5 s or the
  server reports `X-Live-Viewers > 0`; otherwise one frame per second.
- **Wi-Fi**: all-channel scan joining the strongest node (mesh networks share one
  SSID); after 15 consecutive failed uploads the board re-joins Wi-Fi, after 60 it
  restarts. A board stuck on a weak, unreachable mesh node (−73 dBm, no route to the
  host) recovered to −45 dBm after this change.

Measured on the ESP-32S/OV3660 target (AI-Thinker pin map, USB-serial adapter power,
mesh Wi-Fi, ~100 ms average RTT): **7–10 fps at ~4.5 KB per QVGA frame**, stable
for 3 minutes with no stalls or reboots, internal heap ~116 KB free. VGA (~15 KB)
gave only 3–5 fps and visible stalls. Throughput is RTT-bound: each frame waits
for its HTTP response. At the default 19.5 dBm TX power with a 240 MHz CPU the
same board hit the brownout detector; use a solid 5 V supply before raising either.
Other boards, networks and supplies will differ.

The included **AI-Thinker reference pin map** is only a labeled wiring example;
it is not silently assumed to match this target. Its usual OV2640 sensor is not
the user's OV3660. This build's 4 MB flash/PSRAM settings must also be checked
against the physical target. A verified schematic or actual authorized sensor
initialization/capture test is needed to establish compatibility.

## Explicit board selection

There is no automatic/default camera board selection: an unconfigured build
fails with an explanatory message. Use `-DXLENS_BOARD=AI_THINKER` only for the
reference map, or `-DXLENS_BOARD=CUSTOM` after verifying the target board routing. CUSTOM
requires `firmware/main/board_pins.local.h` (ignored by Git) defining all the
`XLENS_PIN_*` macros listed in `board_pins.h` from a verified schematic. Do not
copy the reference values and rename the board. Once a real mapping is verified,
it can be contributed as a separately named, reviewed board profile.
Sensor expectation is separate from pin mapping: default `-DXLENS_SENSOR=OV3660`;
`-DXLENS_SENSOR=OV2640` is available only for an explicitly chosen reference
sensor. Do not relabel a pin map to make it appear verified.

## Dependencies and build

ESP-IDF **5.4.2**, including FreeRTOS/Wi-Fi/lwIP/mbedTLS/NVS, and Espressif
`esp32-camera` **2.1.3** (pinned in `main/idf_component.yml`). The driver’s `esp_jpeg` dependency is pinned to **1.3.2**. These are necessary
platform dependencies, not a dependency-free firmware claim. Review/update pins
for upstream security fixes before an actual deployment. Downloaded vendor code
is not vendored into this repository. Vendor licenses remain their own.

Build the labeled AI-Thinker reference only, with an installed official ESP-IDF
environment (this validates compilation, not the physical target routing):

```sh
. "$IDF_PATH/export.sh"
cd firmware
idf.py -DXLENS_BOARD=AI_THINKER set-target esp32
idf.py -DXLENS_BOARD=AI_THINKER build
idf.py size
```

Alternatively use `espressif/idf:v5.4.2` for a build-only container. The local
development environment could not unpack that image (Docker storage exhausted);
A reference firmware build passed in [GitHub Actions](https://github.com/jveliz-08/X-Lens/actions/runs/40372042440).
Consult the current Actions run for the selected revision. This does not prove
compatibility with the ESP-32S/OV3660 target wiring. No board was flashed.

## Provisioning (instructions only; no flashing performed)

1. Add `device.front-door=<64 random hex characters>` to the server's mode-600
   config, using OS randomness. Use a unique ID/key and restart the server.
2. Run `python3 tools/provision.py`. It prompts privately for Wi-Fi/device secrets
   and writes `runtime/nvs.secret` with mode 600. Supply the server HTTPS origin
   with a publicly trusted certificate. Custom private CAs require deliberate
   configuration of the ESP-IDF trust bundle; never disable verification.
3. Generate a private NVS binary using the vendor generator:

   ```sh
   umask 077
   python "$IDF_PATH/components/nvs_flash/nvs_partition_generator/nvs_partition_gen.py" \
     generate runtime/nvs.secret runtime/nvs.bin 0x6000
   ```

4. When an authorized operator is ready to flash a real board, follow Espressif's
   board flashing instructions. The supplied partition table places NVS at
   `0x9000`, size `0x6000`. Flash the generated NVS partition as well as firmware.
   **Do not run erase-flash or reset NVS on an enrolled device without a new
   identity or an explicit sequence recovery plan.** Preserve the counter across
   firmware updates. `runtime/nvs.bin` contains secrets; keep it out of Git.

Runtime secrets live in NVS, never source or build flags. NVS errors fail closed;
no automatic erase. Wi-Fi requires WPA2 or better. SNTP must synchronize before
TLS certificate validation. Each failed/unknown upload consumes its sequence;
no camera-side retry queue or paid API key exists.

## Required physical validation checklist

- Verify supply voltage, stable current capacity, board pin map and PSRAM health;
  do not disable brownout detection to hide supply problems.
- Build/boot on the actual board; check sensor identity and JPEG decoding.
- Measure minimum free **internal** heap and largest allocation block during TLS
  handshake, upload and reconnect, separately from PSRAM. Current logs report
  post-upload values only, not the handshake minimum.
- Check framebuffer ownership on failures, Wi-Fi loss, server timeout,
  oversized JPEG, invalid CA/hostname, expired cert, redirect and missing time.
- Verify reserve-counter persistence after power loss and repeated reboot, and
  server restart/replay rejection. Do not repeatedly erase NVS.
- Soak-test at expected lighting/temperature for memory growth and flash wear;
  measure actual capture sizes, latencies and sustained fps.

Official references:
- [Espressif camera driver and memory constraints](https://github.com/espressif/esp32-camera)
- [Pinned camera component](https://components.espressif.com/components/espressif/esp32-camera/versions/2.1.3/readme)
- [ESP-IDF HTTP client and HTTPS](https://docs.espressif.com/projects/esp-idf/en/v5.4.2/esp32/api-reference/protocols/esp_http_client.html)
- [ESP-IDF certificate bundle](https://docs.espressif.com/projects/esp-idf/en/v5.4.2/esp32/api-reference/protocols/esp_crt_bundle.html)
- [Espressif mbedTLS memory FAQ](https://docs.espressif.com/projects/esp-faq/en/latest/software-framework/protocols/mbedtls.html)

PSRAM is generally needed above CIF JPEG; this firmware explicitly places its
two framebuffers in PSRAM. RGB/YUV plus Wi-Fi increases memory bandwidth
pressure. `CONFIG_CAMERA_PSRAM_DMA` is for S2/S3, not this classic ESP32 target.

# ESP32-CAM firmware

Target: classic AI-Thinker ESP32-CAM with OV2640, 4 MB flash and working PSRAM.
Other boards need a reviewed pin map. Default: 320×240 JPEG, quality 16, one
PSRAM framebuffer, capture/upload/return, at least five seconds between cycles.
There is no claimed FPS or measured hardware heap budget.

## Dependencies and build

ESP-IDF **5.4.2**, including FreeRTOS/Wi-Fi/lwIP/mbedTLS/NVS, and Espressif
`esp32-camera` **2.1.3** (pinned in `main/idf_component.yml`). These are necessary
platform dependencies, not a dependency-free firmware claim. Review/update pins
for upstream security fixes before an actual deployment. Downloaded vendor code
is not vendored into this repository. Vendor licenses remain their own.

With an installed official ESP-IDF environment:

```sh
. "$IDF_PATH/export.sh"
cd firmware
idf.py set-target esp32
idf.py build
idf.py size
```

Alternatively use `espressif/idf:v5.4.2` for a build-only container. The local
development environment could not unpack that image (Docker storage exhausted);
GitHub Actions includes a clean firmware build. Consult the current Actions run
rather than inferring build success from source presence. No board was flashed.

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
- Check one-framebuffer ownership on failures, Wi-Fi loss, server timeout,
  oversized JPEG, invalid CA/hostname, expired cert, redirect and missing time.
- Verify reserve-counter persistence after power loss and repeated reboot, and
  server restart/replay rejection. Do not repeatedly erase NVS.
- Soak-test at expected lighting/temperature for memory growth and flash wear;
  measure actual capture sizes, latencies and useful snapshot cadence.

Official references:
- [Espressif camera driver and memory constraints](https://github.com/espressif/esp32-camera)
- [Pinned camera component](https://components.espressif.com/components/espressif/esp32-camera/versions/2.1.3/readme)
- [ESP-IDF HTTP client and HTTPS](https://docs.espressif.com/projects/esp-idf/en/v5.4.2/esp32/api-reference/protocols/esp_http_client.html)
- [ESP-IDF certificate bundle](https://docs.espressif.com/projects/esp-idf/en/v5.4.2/esp32/api-reference/protocols/esp_crt_bundle.html)
- [Espressif mbedTLS memory FAQ](https://docs.espressif.com/projects/esp-faq/en/latest/software-framework/protocols/mbedtls.html)

PSRAM is generally needed above CIF JPEG; this firmware explicitly places its
single QVGA framebuffer in PSRAM. RGB/YUV plus Wi-Fi increases memory bandwidth
pressure. `CONFIG_CAMERA_PSRAM_DMA` is for S2/S3, not this classic ESP32 target.

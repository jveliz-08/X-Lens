/* X-Lens: explicitly selected ESP32 camera pin map. No credentials in the binary. */
#include "board_pins.h"
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <ctype.h>
#include <strings.h>
#include <time.h>
#include "freertos/FreeRTOS.h"
#include "freertos/event_groups.h"
#include "freertos/queue.h"
#include "freertos/task.h"
#include "esp_camera.h"
#include "esp_crt_bundle.h"
#include "esp_event.h"
#include "esp_heap_caps.h"
#include "esp_http_client.h"
#include "esp_log.h"
#include "esp_netif.h"
#include "esp_sntp.h"
#include "esp_timer.h"
#include "esp_wifi.h"
#include "jpeg_decoder.h"
#include "nvs_flash.h"

#define MAX_JPEG (128 * 1024)
/* Continuous feed: NVS is written once per block, not per frame (~7 min at 10 fps). */
#define SEQUENCE_BLOCK 4096
/* <= ~15 fps; the server's per-device token bucket refills at 20/s. */
#define MIN_FRAME_MS 66
#define FRAME_SIZE FRAMESIZE_QVGA
#define JPEG_QUALITY 12
#define MAX_TX_POWER 60 /* 0.25 dBm units: 15 dBm */
/* Demand-driven rate: full speed only while someone watches or the scene moves. */
#define IDLE_FRAME_MS 1000
#define MOTION_HOLD_MS 5000
#define MOTION_ACTIVE 30   /* per-mille of changed cells that counts as motion */
#define MOTION_DIFF 18     /* luma change (0-255) that marks a 1/8-scale cell as changed */
#define MOTION_CELLS 1200  /* 40x30 cells for QVGA at 1/8 scale */
/* Consecutive failed uploads before re-joining Wi-Fi (maybe another mesh node), then rebooting. */
/* Auto-exposure settles after (re)starting the sensor; that is not motion. */
#define MOTION_WARMUP_MS 4000
#define FAILS_REJOIN 15
#define FAILS_REBOOT 60
#define CONNECTED BIT0
static const char *TAG = "x-lens";
static EventGroupHandle_t network;
static nvs_handle_t store;
static char ssid[33], password[65], base[161], id[25], key[65];
static uint64_t next_sequence, reserved;
static char url[224], auth[72];
static esp_http_client_handle_t client;
static volatile int viewers;
/* Operator ignore zones from the server (X-Motion-Mask): 8x6 grid, bit row*8+col.
 * Written by the uploader, read by capture; a torn read only affects one frame's score. */
#define ZONE_COLS 8
#define ZONE_ROWS 6
static volatile uint64_t ignore_mask;
/* Capture (core 1) hands scored frames to the uploader (main task) one at a time. */
typedef struct { camera_fb_t *fb; int motion; } shot_t;
static QueueHandle_t shots;
static volatile unsigned decode_us, decoded, motion_peak;
static uint8_t cells[MOTION_CELLS * 3];
static uint16_t background[MOTION_CELLS]; /* luma x16, exponential moving average */
static size_t background_cells;

static void load(const char *name, char *out, size_t capacity) {
    ESP_ERROR_CHECK(nvs_get_str(store, name, out, &capacity));
}
/* Persist a reservation covering `next` before that sequence is ever sent. */
static void reserve_through(uint64_t next) {
    if (next <= reserved) return;
    if (next > UINT64_MAX - SEQUENCE_BLOCK) ESP_ERROR_CHECK(ESP_ERR_INVALID_STATE);
    uint64_t end = next - 1 + SEQUENCE_BLOCK;
    ESP_ERROR_CHECK(nvs_set_u64(store, "reserved", end));
    ESP_ERROR_CHECK(nvs_commit(store));
    reserved = end;
}
static void wifi_event(void *arg, esp_event_base_t event, int32_t number, void *data) {
    (void)arg; (void)data;
    if (event == WIFI_EVENT && number == WIFI_EVENT_STA_START) esp_wifi_connect();
    if (event == WIFI_EVENT && number == WIFI_EVENT_STA_DISCONNECTED) {
        xEventGroupClearBits(network, CONNECTED);
        esp_wifi_connect();
    }
    if (event == IP_EVENT && number == IP_EVENT_STA_GOT_IP) xEventGroupSetBits(network, CONNECTED);
}
static void connect_wifi(void) {
    network = xEventGroupCreate();
    if (!network) ESP_ERROR_CHECK(ESP_ERR_NO_MEM);
    ESP_ERROR_CHECK(esp_netif_init());
    ESP_ERROR_CHECK(esp_event_loop_create_default());
    if (!esp_netif_create_default_wifi_sta()) ESP_ERROR_CHECK(ESP_ERR_NO_MEM);
    wifi_init_config_t init = WIFI_INIT_CONFIG_DEFAULT();
    ESP_ERROR_CHECK(esp_wifi_init(&init));
    ESP_ERROR_CHECK(esp_event_handler_register(WIFI_EVENT, ESP_EVENT_ANY_ID, wifi_event, NULL));
    ESP_ERROR_CHECK(esp_event_handler_register(IP_EVENT, IP_EVENT_STA_GOT_IP, wifi_event, NULL));
    wifi_config_t cfg = {0};
    memcpy(cfg.sta.ssid, ssid, strlen(ssid));
    memcpy(cfg.sta.password, password, strlen(password));
    cfg.sta.threshold.authmode = WIFI_AUTH_WPA2_PSK;
    /* Mesh networks share one SSID across nodes: scan every channel and join the strongest,
     * not the first node found (the default fast scan picked distant, unreachable nodes). */
    cfg.sta.scan_method = WIFI_ALL_CHANNEL_SCAN;
    cfg.sta.sort_method = WIFI_CONNECT_AP_BY_SIGNAL;
    ESP_ERROR_CHECK(esp_wifi_set_storage(WIFI_STORAGE_RAM));
    ESP_ERROR_CHECK(esp_wifi_set_mode(WIFI_MODE_STA));
    ESP_ERROR_CHECK(esp_wifi_set_config(WIFI_IF_STA, &cfg));
    ESP_ERROR_CHECK(esp_wifi_start());
    /* Modem sleep adds 100+ ms latency per frame; this board is mains/USB powered. */
    ESP_ERROR_CHECK(esp_wifi_set_ps(WIFI_PS_NONE));
    /* Continuous TX at the 19.5 dBm default browned out a USB-adapter-powered board. */
    ESP_ERROR_CHECK(esp_wifi_set_max_tx_power(MAX_TX_POWER));
    xEventGroupWaitBits(network, CONNECTED, pdFALSE, pdTRUE, portMAX_DELAY);
}
static void init_camera(void) {
    camera_config_t c = {
        .pin_pwdn=XLENS_PIN_PWDN, .pin_reset=XLENS_PIN_RESET,
        .pin_xclk=XLENS_PIN_XCLK, .pin_sccb_sda=XLENS_PIN_SDA,
        .pin_sccb_scl=XLENS_PIN_SCL, .pin_d7=XLENS_PIN_D7,
        .pin_d6=XLENS_PIN_D6, .pin_d5=XLENS_PIN_D5, .pin_d4=XLENS_PIN_D4,
        .pin_d3=XLENS_PIN_D3, .pin_d2=XLENS_PIN_D2, .pin_d1=XLENS_PIN_D1,
        .pin_d0=XLENS_PIN_D0, .pin_vsync=XLENS_PIN_VSYNC,
        .pin_href=XLENS_PIN_HREF, .pin_pclk=XLENS_PIN_PCLK,
        .xclk_freq_hz=20000000, .ledc_timer=LEDC_TIMER_0,
        .ledc_channel=LEDC_CHANNEL_0, .pixel_format=PIXFORMAT_JPEG,
        .frame_size=FRAME_SIZE, .jpeg_quality=JPEG_QUALITY, .fb_count=2,
        .fb_location=CAMERA_FB_IN_PSRAM, .grab_mode=CAMERA_GRAB_LATEST
    };
    ESP_ERROR_CHECK(esp_camera_init(&c));
    sensor_t *sensor = esp_camera_sensor_get();
    if (!sensor) ESP_ERROR_CHECK(ESP_ERR_NOT_FOUND);
    ESP_LOGI(TAG, "Detected sensor PID=0x%04x; expected=0x%04x", sensor->id.PID, XLENS_EXPECTED_PID);
    if (sensor->id.PID != XLENS_EXPECTED_PID) {
        ESP_LOGE(TAG, "Sensor mismatch; verify hardware and selected profile before capture");
        ESP_ERROR_CHECK(ESP_ERR_NOT_SUPPORTED);
    }
}
/* Edge motion: decode only DC coefficients (1/8 scale) and compare with an adaptive
 * background. A global brightness shift (auto-exposure, lights) is not motion.
 * Returns 0..1000 changed cells per mille, or -1 if the frame cannot be decoded. */
static int motion_score(const camera_fb_t *frame) {
    esp_jpeg_image_cfg_t cfg = {
        .indata=frame->buf, .indata_size=frame->len, .outbuf=cells, .outbuf_size=sizeof(cells),
        .out_format=JPEG_IMAGE_FORMAT_RGB888, .out_scale=JPEG_IMAGE_SCALE_1_8
    };
    esp_jpeg_image_output_t out = {0};
    if (esp_jpeg_decode(&cfg,&out) != ESP_OK) return -1;
    size_t n = (size_t)out.width * out.height;
    if (n == 0 || n > MOTION_CELLS) return -1;
    static uint8_t luma[MOTION_CELLS];
    int32_t shift = 0;
    for (size_t i = 0; i < n; ++i) {
        luma[i] = (uint8_t)((77*cells[3*i] + 150*cells[3*i+1] + 29*cells[3*i+2]) >> 8);
        shift += luma[i] - background[i]/16;
    }
    if (background_cells != n) {
        for (size_t i = 0; i < n; ++i) background[i] = luma[i]*16;
        background_cells = n;
        return 0;
    }
    shift /= (int32_t)n;
    uint64_t mask = ignore_mask;
    unsigned changed = 0, watched = 0;
    for (size_t i = 0; i < n; ++i) {
        int d = luma[i] - background[i]/16 - shift;
        size_t x = i % out.width, y = i / out.width;
        unsigned zone = (unsigned)(y * ZONE_ROWS / out.height) * ZONE_COLS + (unsigned)(x * ZONE_COLS / out.width);
        if (!(mask >> zone & 1)) {
            watched++;
            if (d > MOTION_DIFF || d < -MOTION_DIFF) changed++;
        }
        background[i] = (uint16_t)(background[i] + (luma[i]*16 - background[i])/8);
    }
    return watched ? (int)(changed*1000/watched) : 0;
}
static esp_err_t on_http_event(esp_http_client_event_t *e) {
    if (e->event_id == HTTP_EVENT_ON_HEADER && strcasecmp(e->header_key,"X-Live-Viewers") == 0)
        viewers = atoi(e->header_value);
    if (e->event_id == HTTP_EVENT_ON_HEADER && strcasecmp(e->header_key,"X-Motion-Mask") == 0 &&
        strlen(e->header_value) == 12)
        ignore_mask = strtoull(e->header_value, NULL, 16) & ((1ULL << (ZONE_COLS*ZONE_ROWS)) - 1);
    return ESP_OK;
}
static esp_http_client_handle_t open_client(void) {
    esp_http_client_config_t cfg = {
        .url=url, .method=HTTP_METHOD_POST, .timeout_ms=5000, .event_handler=on_http_event,
        .crt_bundle_attach=esp_crt_bundle_attach,
        .disable_auto_redirect=true, .buffer_size=1024, .buffer_size_tx=1024
    };
    esp_http_client_handle_t c = esp_http_client_init(&cfg);
    if (c && (esp_http_client_set_header(c,"Authorization",auth) != ESP_OK ||
              esp_http_client_set_header(c,"X-Protocol","1") != ESP_OK ||
              esp_http_client_set_header(c,"Content-Type","image/jpeg") != ESP_OK)) {
        esp_http_client_cleanup(c);
        c = NULL;
    }
    return c;
}
/* One persistent HTTPS connection: the TLS handshake is paid once, not per frame. */
static int upload(camera_fb_t *frame, uint64_t seq, int motion) {
    if (!client && !(client = open_client())) return -1;
    char number[21], level[12];
    snprintf(number, sizeof(number), "%" PRIu64, seq);
    snprintf(level, sizeof(level), "%d", motion < 0 ? 0 : motion);
    if (esp_http_client_set_header(client,"X-Sequence",number) != ESP_OK ||
        esp_http_client_set_header(client,"X-Motion",level) != ESP_OK ||
        esp_http_client_set_post_field(client,(const char *)frame->buf,(int)frame->len) != ESP_OK ||
        esp_http_client_perform(client) != ESP_OK) {
        /* Outcome unknown: drop the connection; the sequence stays consumed. */
        esp_http_client_cleanup(client);
        client = NULL;
        return -1;
    }
    return esp_http_client_get_status_code(client);
}
/* After 409, adopt the server's next sequence (e.g. after a counter rollback). Never moves backwards. */
static void resync(void) {
    char state_url[224], body[160] = {0};
    snprintf(state_url, sizeof(state_url), "%s/api/device-state/%s", base, id);
    esp_http_client_config_t cfg = {
        .url=state_url, .timeout_ms=5000, .crt_bundle_attach=esp_crt_bundle_attach,
        .disable_auto_redirect=true
    };
    esp_http_client_handle_t c = esp_http_client_init(&cfg);
    if (!c) return;
    int len = -1;
    if (esp_http_client_set_header(c,"Authorization",auth) == ESP_OK &&
        esp_http_client_set_header(c,"X-Protocol","1") == ESP_OK &&
        esp_http_client_open(c,0) == ESP_OK && esp_http_client_fetch_headers(c) >= 0 &&
        esp_http_client_get_status_code(c) == 200)
        len = esp_http_client_read_response(c,body,sizeof(body)-1);
    esp_http_client_cleanup(c);
    const char *field = len > 0 ? strstr(body,"\"next_sequence\":\"") : NULL;
    if (!field) return;
    uint64_t next = strtoull(field+17,NULL,10);
    if (next <= next_sequence || next > UINT64_MAX - SEQUENCE_BLOCK) return;
    reserve_through(next);
    ESP_LOGW(TAG,"Sequence resynchronized %" PRIu64 " -> %" PRIu64,next_sequence,next);
    next_sequence = next;
}
/* Core 1: capture and score the next frame while the main task uploads the previous one,
 * so the 1/8-scale decode costs no frame time. Two framebuffers make this possible. */
static void capture_task(void *arg) {
    (void)arg;
    const int64_t warm = esp_timer_get_time() + MOTION_WARMUP_MS*1000LL;
    int64_t last = 0, moved = 0;
    for (;;) {
        bool active = viewers > 0 || esp_timer_get_time()-moved < MOTION_HOLD_MS*1000LL;
        int64_t wait_ms = (active ? MIN_FRAME_MS : IDLE_FRAME_MS) - (esp_timer_get_time()-last)/1000;
        vTaskDelay(pdMS_TO_TICKS(wait_ms > 0 ? wait_ms : 0) + 1); /* Always yield to IDLE (watchdog). */
        last = esp_timer_get_time();
        camera_fb_t *frame = esp_camera_fb_get();
        if (!frame) {
            ESP_LOGW(TAG,"Capture unavailable");
            vTaskDelay(pdMS_TO_TICKS(1000));
            continue;
        }
        if (frame->format != PIXFORMAT_JPEG || frame->len < 4 || frame->len > MAX_JPEG) {
            ESP_LOGW(TAG,"Dropped oversized or non-JPEG frame");
            esp_camera_fb_return(frame);
            continue;
        }
        int64_t t = esp_timer_get_time();
        int motion = motion_score(frame); /* Always run: the background keeps adapting. */
        decode_us += (unsigned)(esp_timer_get_time()-t);
        decoded++;
        if (t < warm && motion > 0) motion = 0;
        if (motion >= MOTION_ACTIVE) moved = esp_timer_get_time();
        if (motion > (int)motion_peak) motion_peak = (unsigned)motion;
        shot_t shot = {frame, motion};
        xQueueSend(shots,&shot,portMAX_DELAY); /* Waits while the uploader holds the previous frame. */
    }
}
void app_main(void) {
    /* Fail closed on NVS corruption. Never silently erase credentials/counters. */
    ESP_ERROR_CHECK(nvs_flash_init());
    ESP_ERROR_CHECK(nvs_open("xlens",NVS_READWRITE,&store));
    load("ssid",ssid,sizeof(ssid)); load("password",password,sizeof(password));
    load("base",base,sizeof(base)); load("id",id,sizeof(id)); load("token",key,sizeof(key));
    if (!ssid[0] || strlen(password)<8 || !id[0] || strlen(key)!=64 ||
        strncmp(base,"https://",8)!=0 || !base[8]) ESP_ERROR_CHECK(ESP_ERR_INVALID_ARG);
    for (const char *p=id; *p; ++p) if (!((*p>='a'&&*p<='z')||(*p>='0'&&*p<='9')||*p=='-')) ESP_ERROR_CHECK(ESP_ERR_INVALID_ARG);
    for (const char *p=key; *p; ++p) if (!isxdigit((unsigned char)*p)) ESP_ERROR_CHECK(ESP_ERR_INVALID_ARG);
    for (const char *p=base+8; *p; ++p) if (!isalnum((unsigned char)*p)&&*p!='.'&&*p!='-'&&*p!=':') ESP_ERROR_CHECK(ESP_ERR_INVALID_ARG);
    ESP_ERROR_CHECK(nvs_get_u64(store,"reserved",&reserved));
    /* A reboot skips the rest of the previous block; unused numbers are never reused. */
    if (reserved > UINT64_MAX - SEQUENCE_BLOCK) ESP_ERROR_CHECK(ESP_ERR_INVALID_STATE);
    next_sequence = reserved + 1;
    reserve_through(next_sequence);
    snprintf(url, sizeof(url), "%s/api/frame/%s", base, id);
    snprintf(auth, sizeof(auth), "Bearer %s", key);
    connect_wifi();
    esp_sntp_setoperatingmode(SNTP_OPMODE_POLL);
    esp_sntp_setservername(0,"pool.ntp.org");
    esp_sntp_init();
    /* Certificate validity requires time; do not disable verification on failure. */
    while (sntp_get_sync_status() == SNTP_SYNC_STATUS_RESET) vTaskDelay(pdMS_TO_TICKS(1000));
    init_camera();
    shots = xQueueCreate(1, sizeof(shot_t));
    if (!shots || xTaskCreatePinnedToCore(capture_task,"capture",6144,NULL,5,NULL,1) != pdPASS)
        ESP_ERROR_CHECK(ESP_ERR_NO_MEM);
    int64_t window = esp_timer_get_time();
    unsigned sent = 0, bytes = 0, failures = 0;
    int status = 0;
    for (;;) {
        xEventGroupWaitBits(network,CONNECTED,pdFALSE,pdTRUE,portMAX_DELAY);
        shot_t shot;
        if (xQueueReceive(shots,&shot,pdMS_TO_TICKS(2000)) == pdTRUE) {
            reserve_through(next_sequence);
            /* A lost response is unknown; consume sequence, never replay this frame. */
            status = upload(shot.fb,next_sequence++,shot.motion);
            if (status == 201) { sent++; bytes += shot.fb->len; }
            esp_camera_fb_return(shot.fb);
        }
        failures = status < 0 ? failures + 1 : 0;
        if (failures == FAILS_REJOIN) {
            ESP_LOGW(TAG,"Server unreachable; re-joining Wi-Fi");
            esp_wifi_disconnect(); /* The disconnect handler reconnects to the strongest node. */
        } else if (failures >= FAILS_REBOOT) {
            ESP_LOGE(TAG,"Server unreachable; restarting");
            esp_restart();
        }
        if (status == 409) resync();
        else if (status < 0 || status == 429 || status >= 500) vTaskDelay(pdMS_TO_TICKS(1000));
        int64_t elapsed = esp_timer_get_time()-window;
        if (elapsed >= 5000000) {
            wifi_ap_record_t ap;
            int rssi = esp_wifi_sta_get_ap_info(&ap) == ESP_OK ? ap.rssi : 0;
            unsigned n = decoded;
            ESP_LOGI(TAG,"fps=%u.%u avg_bytes=%u motion_peak=%u decode_us=%u viewers=%d status=%d seq=%" PRIu64 " rssi=%d internal_free=%u largest=%u",
                     (unsigned)(sent*10000000LL/elapsed/10),(unsigned)(sent*10000000LL/elapsed%10),
                     sent ? bytes/sent : 0,motion_peak,n ? decode_us/n : 0,viewers,status,next_sequence-1,rssi,
                     (unsigned)heap_caps_get_free_size(MALLOC_CAP_INTERNAL|MALLOC_CAP_8BIT),
                     (unsigned)heap_caps_get_largest_free_block(MALLOC_CAP_INTERNAL|MALLOC_CAP_8BIT));
            window += elapsed; sent = 0; bytes = 0; motion_peak = 0; decode_us = 0; decoded = 0;
        }
    }
}

/* X-Lens: AI-Thinker ESP32-CAM / OV2640. No credentials in the binary. */
#include <inttypes.h>
#include <stdio.h>
#include <string.h>
#include <ctype.h>
#include <time.h>
#include "freertos/FreeRTOS.h"
#include "freertos/event_groups.h"
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
#include "nvs_flash.h"

#define MAX_JPEG (128 * 1024)
#define SEQUENCE_BLOCK 256
#define CONNECTED BIT0
static const char *TAG = "x-lens";
static EventGroupHandle_t network;
static nvs_handle_t store;
static char ssid[33], password[65], base[161], id[25], key[65];
static uint64_t next_sequence, reserved;

static void load(const char *name, char *out, size_t capacity) {
    ESP_ERROR_CHECK(nvs_get_str(store, name, out, &capacity));
}
static void reserve_sequences(void) {
    if (reserved > UINT64_MAX - SEQUENCE_BLOCK) ESP_ERROR_CHECK(ESP_ERR_INVALID_STATE);
    uint64_t end = reserved + SEQUENCE_BLOCK;
    ESP_ERROR_CHECK(nvs_set_u64(store, "reserved", end));
    ESP_ERROR_CHECK(nvs_commit(store));
    next_sequence = reserved + 1;
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
    ESP_ERROR_CHECK(esp_wifi_set_storage(WIFI_STORAGE_RAM));
    ESP_ERROR_CHECK(esp_wifi_set_mode(WIFI_MODE_STA));
    ESP_ERROR_CHECK(esp_wifi_set_config(WIFI_IF_STA, &cfg));
    ESP_ERROR_CHECK(esp_wifi_start());
    xEventGroupWaitBits(network, CONNECTED, pdFALSE, pdTRUE, portMAX_DELAY);
}
static void init_camera(void) {
    camera_config_t c = {
        .pin_pwdn=32, .pin_reset=-1, .pin_xclk=0,
        .pin_sccb_sda=26, .pin_sccb_scl=27,
        .pin_d7=35, .pin_d6=34, .pin_d5=39, .pin_d4=36,
        .pin_d3=21, .pin_d2=19, .pin_d1=18, .pin_d0=5,
        .pin_vsync=25, .pin_href=23, .pin_pclk=22,
        .xclk_freq_hz=20000000, .ledc_timer=LEDC_TIMER_0,
        .ledc_channel=LEDC_CHANNEL_0, .pixel_format=PIXFORMAT_JPEG,
        .frame_size=FRAMESIZE_QVGA, .jpeg_quality=16, .fb_count=1,
        .fb_location=CAMERA_FB_IN_PSRAM, .grab_mode=CAMERA_GRAB_WHEN_EMPTY
    };
    ESP_ERROR_CHECK(esp_camera_init(&c));
}
static int upload(camera_fb_t *frame, uint64_t seq) {
    char url[224], auth[72], number[21];
    snprintf(url, sizeof(url), "%s/api/frame/%s", base, id);
    snprintf(auth, sizeof(auth), "Bearer %s", key);
    snprintf(number, sizeof(number), "%" PRIu64, seq);
    esp_http_client_config_t cfg = {
        .url=url, .method=HTTP_METHOD_POST, .timeout_ms=5000,
        .crt_bundle_attach=esp_crt_bundle_attach,
        .disable_auto_redirect=true, .buffer_size=1024, .buffer_size_tx=1024
    };
    esp_http_client_handle_t client = esp_http_client_init(&cfg);
    if (!client) return -1;
    int status = -1;
    if (esp_http_client_set_header(client,"Authorization",auth) != ESP_OK ||
        esp_http_client_set_header(client,"Content-Type","image/jpeg") != ESP_OK ||
        esp_http_client_set_header(client,"X-Sequence",number) != ESP_OK ||
        esp_http_client_open(client,(int)frame->len) != ESP_OK) goto done;
    size_t offset = 0;
    int64_t start = esp_timer_get_time();
    while (offset < frame->len) {
        if (esp_timer_get_time()-start > 10000000) goto done;
        size_t chunk = frame->len-offset;
        if (chunk > 4096) chunk = 4096;
        int n = esp_http_client_write(client,(const char *)frame->buf+offset,(int)chunk);
        if (n <= 0) goto done;
        offset += (size_t)n;
    }
    if (esp_http_client_fetch_headers(client) >= 0) status = esp_http_client_get_status_code(client);
 done:
    esp_http_client_close(client);
    esp_http_client_cleanup(client);
    return status;
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
    reserve_sequences();
    connect_wifi();
    esp_sntp_setoperatingmode(SNTP_OPMODE_POLL);
    esp_sntp_setservername(0,"pool.ntp.org");
    esp_sntp_init();
    /* Certificate validity requires time; do not disable verification on failure. */
    while (sntp_get_sync_status() == SNTP_SYNC_STATUS_RESET) vTaskDelay(pdMS_TO_TICKS(1000));
    init_camera();
    for (;;) {
        xEventGroupWaitBits(network,CONNECTED,pdFALSE,pdTRUE,portMAX_DELAY);
        if (next_sequence > reserved) reserve_sequences();
        camera_fb_t *frame = esp_camera_fb_get();
        if (frame) {
            if (frame->format == PIXFORMAT_JPEG && frame->len >= 4 && frame->len <= MAX_JPEG) {
                uint64_t seq = next_sequence++;
                int result = upload(frame,seq);
                ESP_LOGI(TAG,"seq=%" PRIu64 " bytes=%u status=%d internal_free=%u largest=%u",seq,(unsigned)frame->len,result,(unsigned)heap_caps_get_free_size(MALLOC_CAP_INTERNAL|MALLOC_CAP_8BIT),(unsigned)heap_caps_get_largest_free_block(MALLOC_CAP_INTERNAL|MALLOC_CAP_8BIT));
                /* A lost response is unknown; consume sequence, never replay this frame. */
            } else ESP_LOGW(TAG,"Dropped oversized or non-JPEG frame");
            esp_camera_fb_return(frame);
        } else ESP_LOGW(TAG,"Capture unavailable");
        vTaskDelay(pdMS_TO_TICKS(5000));
    }
}

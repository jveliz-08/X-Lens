/* Explicit board selection. Never infer a pin map from the label "regular". */
#pragma once
#if defined(XLENS_AI_THINKER_REFERENCE)
/* Reference example only; not a claim about the user's unidentified board. */
#define XLENS_PIN_PWDN 32
#define XLENS_PIN_RESET -1
#define XLENS_PIN_XCLK 0
#define XLENS_PIN_SDA 26
#define XLENS_PIN_SCL 27
#define XLENS_PIN_D7 35
#define XLENS_PIN_D6 34
#define XLENS_PIN_D5 39
#define XLENS_PIN_D4 36
#define XLENS_PIN_D3 21
#define XLENS_PIN_D2 19
#define XLENS_PIN_D1 18
#define XLENS_PIN_D0 5
#define XLENS_PIN_VSYNC 25
#define XLENS_PIN_HREF 23
#define XLENS_PIN_PCLK 22
#elif defined(XLENS_CUSTOM_BOARD)
#include "board_pins.local.h"
#else
#error "Select an identified board explicitly with -DXLENS_BOARD=AI_THINKER or CUSTOM"
#endif

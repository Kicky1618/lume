#pragma once

#include <stddef.h>
#include <stdint.h>

typedef struct {
  float x;
  float y;
} Vec2;

typedef struct Canvas Canvas;
typedef struct Decoder Decoder;

typedef enum {
  IMAGE_FORMAT_PNG = 0,
  IMAGE_FORMAT_JPEG = 1,
  IMAGE_FORMAT_WEBP = 2,
} ImageFormat;

typedef struct {
  const uint8_t* ptr;
  size_t len;
} lume_bytes;

int canvas_create(Vec2 size, Canvas** out_canvas);
void canvas_destroy(Canvas* canvas);

int decoder_new(const uint8_t* bytes_ptr, size_t bytes_len, Decoder** out_decoder);
void decoder_free(Decoder* decoder);

int decode(const uint8_t* bytes_ptr, size_t bytes_len, lume_bytes* out_bytes);
void bytes_free(uint8_t* bytes_ptr);

int render(Canvas* canvas, Decoder* image, ImageFormat format, lume_bytes* out_bytes);
int mandelbrot_render(int32_t width,
                      int32_t height,
                      int32_t max_iterations,
                      double center_x,
                      double center_y,
                      double scale,
                      lume_bytes* out_bytes);
void onProgress(uint64_t current, uint64_t total);

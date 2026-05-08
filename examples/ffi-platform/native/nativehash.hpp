#pragma once

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
  uint8_t *ptr;
  size_t len;
} lume_bytes;

int hash(const uint8_t *input_ptr, size_t input_len, lume_bytes *out_bytes);
int hash_preview_rgba(int32_t width, int32_t height, const uint8_t *input_ptr, size_t input_len, lume_bytes *out_bytes);
void bytes_free(uint8_t *bytes_ptr);

#ifdef __cplusplus
}
#endif

#include "../nativehash.hpp"

#include <cstddef>
#include <cstdint>
#include <cstdlib>

static unsigned long long hash_bytes(const uint8_t *bytes_ptr, size_t bytes_len) {
  unsigned long long value = 1469598103934665603ULL;
  for (size_t i = 0; i < bytes_len; ++i) {
    value ^= bytes_ptr ? bytes_ptr[i] : 0;
    value *= 1099511628211ULL;
  }
  return value;
}

void bytes_free(uint8_t *bytes_ptr) {
  std::free(bytes_ptr);
}

extern "C" int hash(const uint8_t *input_ptr,
                    size_t input_len,
                    lume_bytes *out_bytes) {
  if (out_bytes == nullptr) {
    return 1;
  }

  out_bytes->ptr = nullptr;
  out_bytes->len = 0;

  uint8_t *digest = static_cast<uint8_t *>(std::malloc(8));
  if (digest == nullptr) {
    return 2;
  }

  const unsigned long long value = hash_bytes(input_ptr, input_len);
  for (size_t index = 0; index < 8; ++index) {
    digest[index] = static_cast<uint8_t>((value >> ((7 - index) * 8)) & 0xFFu);
  }

  out_bytes->ptr = digest;
  out_bytes->len = 8;
  return 0;
}

extern "C" int hash_preview_rgba(int32_t width,
                                 int32_t height,
                                 const uint8_t *input_ptr,
                                 size_t input_len,
                                 lume_bytes *out_bytes) {
  if (out_bytes == nullptr || width <= 0 || height <= 0) {
    return 1;
  }

  out_bytes->ptr = nullptr;
  out_bytes->len = 0;

  const size_t pixel_count = static_cast<size_t>(width) * static_cast<size_t>(height);
  if (pixel_count > SIZE_MAX / 4u) {
    return 2;
  }

  const size_t len = pixel_count * 4u;
  uint8_t *pixels = static_cast<uint8_t *>(std::malloc(len));
  if (pixels == nullptr) {
    return 3;
  }

  const unsigned long long seed = hash_bytes(input_ptr, input_len);
  const uint8_t r = static_cast<uint8_t>(seed & 0xFFu);
  const uint8_t g = static_cast<uint8_t>((seed >> 8) & 0xFFu);
  const uint8_t b = static_cast<uint8_t>((seed >> 16) & 0xFFu);

  for (int32_t y = 0; y < height; ++y) {
    for (int32_t x = 0; x < width; ++x) {
      const size_t index = (static_cast<size_t>(y) * static_cast<size_t>(width) + static_cast<size_t>(x)) * 4u;
      const uint8_t shade = static_cast<uint8_t>((x * 255) / (width > 1 ? width - 1 : 1));
      pixels[index + 0] = static_cast<uint8_t>((r / 2) + (shade / 2));
      pixels[index + 1] = static_cast<uint8_t>((g / 2) + (static_cast<uint8_t>(y * 255 / (height > 1 ? height - 1 : 1)) / 2));
      pixels[index + 2] = b;
      pixels[index + 3] = 255;
    }
  }

  out_bytes->ptr = pixels;
  out_bytes->len = len;
  return 0;
}

#include "renderkit.h"

#include <stdlib.h>
#include <stdint.h>
#include <stddef.h>
#include <stdlib.h>
#include <string.h>
#include <limits.h>

int canvas_create(Vec2 size, Canvas **out_canvas)
{
  (void)size;
  (void)out_canvas;
  return 0;
}

void canvas_destroy(Canvas *canvas)
{
  (void)canvas;
}

int decoder_new(const uint8_t *bytes_ptr, size_t bytes_len, Decoder **out_decoder)
{
  (void)bytes_ptr;
  (void)bytes_len;
  (void)out_decoder;
  return 0;
}

void decoder_free(Decoder *decoder)
{
  (void)decoder;
}

int decode(const uint8_t *bytes_ptr, size_t bytes_len, lume_bytes *out_bytes)
{
  (void)bytes_ptr;
  (void)bytes_len;
  (void)out_bytes;
  return 0;
}

void bytes_free(uint8_t *bytes_ptr)
{
  free(bytes_ptr);
}

int render(Canvas *canvas, Decoder *image, ImageFormat format, lume_bytes *out_bytes)
{
  (void)canvas;
  (void)image;
  (void)format;
  (void)out_bytes;
  return 0;
}
static inline int mandelbrot_inside_fast(double cr, double ci)
{
  const double ci2 = ci * ci;

  /*
    Main cardioid:
      q = (x - 1/4)^2 + y^2
      q(q + x - 1/4) <= y^2 / 4
  */
  const double x = cr - 0.25;
  const double q = x * x + ci2;
  if (q * (q + x) <= 0.25 * ci2)
  {
    return 1;
  }

  /*
    Period-2 bulb:
      (x + 1)^2 + y^2 <= 1/16
  */
  const double xp1 = cr + 1.0;
  if (xp1 * xp1 + ci2 <= 0.0625)
  {
    return 1;
  }

  return 0;
}

static inline void mandelbrot_render_row(
    int32_t width,
    int32_t max_iterations,
    double x0,
    double dx,
    double ci,
    double inv_max_iterations,
    uint8_t *restrict row)
{
  double cr = x0;
  uint8_t *restrict p = row;

  for (int32_t x = 0; x < width; x++)
  {
    if (mandelbrot_inside_fast(cr, ci))
    {
      p[0] = 5;
      p[1] = 7;
      p[2] = 12;
      p[3] = 255;

      cr += dx;
      p += 4;
      continue;
    }

    double zr = 0.0;
    double zi = 0.0;
    double zr2 = 0.0;
    double zi2 = 0.0;

    int32_t iter = 0;

    while (zr2 + zi2 <= 4.0 && iter < max_iterations)
    {
      zi = (zr + zr) * zi + ci;
      zr = zr2 - zi2 + cr;

      zr2 = zr * zr;
      zi2 = zi * zi;

      iter++;
    }

    if (iter == max_iterations)
    {
      p[0] = 5;
      p[1] = 7;
      p[2] = 12;
    }
    else
    {
      const double t = (double)iter * inv_max_iterations;
      const double t2 = t * t;

      p[0] = (uint8_t)(9.0 + 46.0 * t + 180.0 * t2);
      p[1] = (uint8_t)(32.0 + 120.0 * t);
      p[2] = (uint8_t)(92.0 + 140.0 * (1.0 - t));
    }

    p[3] = 255;

    cr += dx;
    p += 4;
  }
}

int mandelbrot_render(int32_t width,
                      int32_t height,
                      int32_t max_iterations,
                      double center_x,
                      double center_y,
                      double scale,
                      lume_bytes *out_bytes)
{
  if (out_bytes == NULL)
  {
    return 1;
  }

  out_bytes->ptr = NULL;
  out_bytes->len = 0;

  if (width <= 0 || height <= 0 || max_iterations <= 0 || scale <= 0.0)
  {
    return 1;
  }

  const size_t w = (size_t)width;
  const size_t h = (size_t)height;

  if (w > SIZE_MAX / h)
  {
    return 3;
  }

  const size_t pixel_count = w * h;

  if (pixel_count > SIZE_MAX / 4u)
  {
    return 3;
  }

  const size_t len = pixel_count * 4u;
  uint8_t *pixels = (uint8_t *)malloc(len);

  if (pixels == NULL)
  {
    return 2;
  }

  const double aspect = (double)width / (double)height;
  const double view_height = 3.0 / scale;
  const double view_width = view_height * aspect;

  const double x0 = width > 1
                        ? center_x - view_width * 0.5
                        : center_x;

  const double y0 = height > 1
                        ? center_y - view_height * 0.5
                        : center_y;

  const double dx = width > 1
                        ? view_width / (double)(width - 1)
                        : 0.0;

  const double dy = height > 1
                        ? view_height / (double)(height - 1)
                        : 0.0;

  const double inv_max_iterations = 1.0 / (double)max_iterations;
  const size_t stride = w * 4u;

  /*
    Mandelbrot は実軸に対して対称。
    center_y == 0.0 のときは上半分だけ描画して下半分へコピーできる。
  */
  const int use_vertical_symmetry = center_y == 0.0 && height > 1;
  const int32_t render_rows = use_vertical_symmetry
                                  ? (height + 1) / 2
                                  : height;

#if defined(_OPENMP)
#pragma omp parallel for schedule(dynamic, 1)
#endif
  for (int32_t y = 0; y < render_rows; y++)
  {
    const double ci = y0 + dy * (double)y;
    uint8_t *row = pixels + (size_t)y * stride;

    mandelbrot_render_row(
        width,
        max_iterations,
        x0,
        dx,
        ci,
        inv_max_iterations,
        row);

    if (use_vertical_symmetry)
    {
      const int32_t mirror_y = height - 1 - y;

      if (mirror_y != y)
      {
        uint8_t *mirror_row = pixels + (size_t)mirror_y * stride;
        memcpy(mirror_row, row, stride);
      }
    }
  }

  out_bytes->ptr = pixels;
  out_bytes->len = len;

  return 0;
}

void onProgress(uint64_t current, uint64_t total)
{
  (void)current;
  (void)total;
}

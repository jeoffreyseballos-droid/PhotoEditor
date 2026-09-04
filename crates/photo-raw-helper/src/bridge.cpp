// Transport only. All RAW identification, calibration and demosaic belong to LibRaw.
#include "libraw/libraw.h"
#include <string>
#ifdef _WIN32
#include <windows.h>
#endif
extern "C" int pe_decode(const char *path, int half, unsigned long long max_pixels,
                         void **owner, const unsigned char **pixels, unsigned *width,
                         unsigned *height, unsigned *bytes, unsigned *warnings) {
  try {
    LibRaw raw;
    raw.imgdata.rawparams.max_raw_memory_mb = 2048;
    auto &p = raw.imgdata.params;
    p.output_color = 1; // linear sRGB primaries; LibRaw camera matrices
    p.output_bps = 16;
    p.gamm[0] = p.gamm[1] = 1.0;
    p.no_auto_bright = 1;
    p.use_camera_wb = 1;
    p.user_qual = 3; // AHD for Bayer; LibRaw dispatches other sensor layouts
    p.half_size = half;
    p.highlight = 2; // blend; not a promise of recovery of clipped sensor values
    int rc;
#ifdef _WIN32
    int count = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, path, -1, nullptr, 0);
    if (!count) return LIBRAW_IO_ERROR;
    std::wstring wide(count, L'\0');
    MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, path, -1, &wide[0], count);
    rc = raw.open_file(wide.c_str());
#else
    rc = raw.open_file(path);
#endif
    if (rc) return rc;
    auto &s = raw.imgdata.sizes;
    if (!s.raw_width || !s.raw_height ||
        static_cast<unsigned long long>(s.raw_width) * s.raw_height > max_pixels)
      return LIBRAW_TOO_BIG;
    if ((rc = raw.unpack()) || (rc = raw.dcraw_process())) return rc;
    libraw_processed_image_t *image = raw.dcraw_make_mem_image(&rc);
    if (!image) return rc ? rc : LIBRAW_UNSPECIFIED_ERROR;
    if (image->type != LIBRAW_IMAGE_BITMAP || image->colors != 3 || image->bits != 16) {
      LibRaw::dcraw_clear_mem(image);
      return LIBRAW_NOT_IMPLEMENTED;
    }
    *owner = image; *pixels = image->data; *width = image->width;
    *height = image->height; *bytes = image->data_size;
    *warnings = raw.imgdata.process_warnings;
    return 0;
  } catch (...) { return LIBRAW_UNSPECIFIED_ERROR; }
}
extern "C" void pe_free(void *owner) {
  LibRaw::dcraw_clear_mem(static_cast<libraw_processed_image_t *>(owner));
}
extern "C" const char *pe_error(int code) { return libraw_strerror(code); }

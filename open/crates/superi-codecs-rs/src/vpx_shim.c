#include "vpx_shim.h"

#include <limits.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include <vpx/vp8cx.h>
#include <vpx/vp8dx.h>
#include <vpx/vpx_codec.h>
#include <vpx/vpx_decoder.h>
#include <vpx/vpx_encoder.h>
#include <vpx/vpx_image.h>

#define SUPERI_VPX_INVALID_ARGUMENT (-1)
#define SUPERI_VPX_OUT_OF_MEMORY (-2)
#define SUPERI_VPX_UNSUPPORTED_FORMAT (-3)
#define SUPERI_VPX_BUFFER_MISMATCH (-4)

typedef const char *(*codec_err_to_string_fn)(vpx_codec_err_t);
typedef const char *(*codec_error_fn)(const vpx_codec_ctx_t *);
typedef vpx_codec_err_t (*codec_control_fn)(vpx_codec_ctx_t *, int, ...);
typedef vpx_codec_iface_t *(*codec_iface_fn)(void);
typedef vpx_codec_err_t (*decoder_init_fn)(
    vpx_codec_ctx_t *, vpx_codec_iface_t *, const vpx_codec_dec_cfg_t *,
    vpx_codec_flags_t, int);
typedef vpx_codec_err_t (*decode_fn)(vpx_codec_ctx_t *, const uint8_t *,
                                    unsigned int, void *, long);
typedef vpx_image_t *(*get_frame_fn)(vpx_codec_ctx_t *, vpx_codec_iter_t *);
typedef vpx_codec_err_t (*encoder_config_default_fn)(vpx_codec_iface_t *,
                                                     vpx_codec_enc_cfg_t *,
                                                     unsigned int);
typedef vpx_codec_err_t (*encoder_init_fn)(vpx_codec_ctx_t *,
                                           vpx_codec_iface_t *,
                                           const vpx_codec_enc_cfg_t *,
                                           vpx_codec_flags_t, int);
typedef vpx_codec_err_t (*encode_fn)(vpx_codec_ctx_t *, const vpx_image_t *,
                                    vpx_codec_pts_t, unsigned long,
                                    vpx_enc_frame_flags_t,
                                    vpx_enc_deadline_t);
typedef const vpx_codec_cx_pkt_t *(*get_packet_fn)(vpx_codec_ctx_t *,
                                                   vpx_codec_iter_t *);
typedef vpx_codec_err_t (*destroy_fn)(vpx_codec_ctx_t *);
typedef vpx_image_t *(*image_alloc_fn)(vpx_image_t *, vpx_img_fmt_t,
                                       unsigned int, unsigned int,
                                       unsigned int);
typedef void (*image_free_fn)(vpx_image_t *);

struct superi_vpx_decoder {
  const struct superi_vpx_api *api;
  vpx_codec_ctx_t context;
  vpx_codec_iter_t iterator;
  vpx_image_t *current;
  int initialized;
};

struct superi_vpx_encoder {
  const struct superi_vpx_api *api;
  vpx_codec_ctx_t context;
  vpx_codec_iter_t iterator;
  int codec;
  int initialized;
};

static vpx_codec_iface_t *decoder_interface(
    const struct superi_vpx_api *api, int codec) {
  codec_iface_fn get_interface;
  if (codec == SUPERI_VPX_CODEC_VP8) {
    get_interface = (codec_iface_fn)api->codec_vp8_dx;
  } else if (codec == SUPERI_VPX_CODEC_VP9) {
    get_interface = (codec_iface_fn)api->codec_vp9_dx;
  } else {
    return NULL;
  }
  return get_interface();
}

static vpx_codec_iface_t *encoder_interface(
    const struct superi_vpx_api *api, int codec) {
  codec_iface_fn get_interface;
  if (codec == SUPERI_VPX_CODEC_VP8) {
    get_interface = (codec_iface_fn)api->codec_vp8_cx;
  } else if (codec == SUPERI_VPX_CODEC_VP9) {
    get_interface = (codec_iface_fn)api->codec_vp9_cx;
  } else {
    return NULL;
  }
  return get_interface();
}

static int image_format(vpx_img_fmt_t format, uint32_t bit_depth) {
  const int high = (format & VPX_IMG_FMT_HIGHBITDEPTH) != 0;
  const vpx_img_fmt_t base = format & ~VPX_IMG_FMT_HIGHBITDEPTH;
  if (!high && bit_depth == 8) {
    if (base == VPX_IMG_FMT_I420) return SUPERI_VPX_FORMAT_I420_8;
    if (base == VPX_IMG_FMT_I422) return SUPERI_VPX_FORMAT_I422_8;
    if (base == VPX_IMG_FMT_I444) return SUPERI_VPX_FORMAT_I444_8;
  }
  if (high && bit_depth == 10) {
    if (base == VPX_IMG_FMT_I420) return SUPERI_VPX_FORMAT_I420_10;
    if (base == VPX_IMG_FMT_I422) return SUPERI_VPX_FORMAT_I422_10;
    if (base == VPX_IMG_FMT_I444) return SUPERI_VPX_FORMAT_I444_10;
  }
  return SUPERI_VPX_UNSUPPORTED_FORMAT;
}

static vpx_img_fmt_t native_image_format(int format) {
  switch (format) {
    case SUPERI_VPX_FORMAT_I420_8:
      return VPX_IMG_FMT_I420;
    case SUPERI_VPX_FORMAT_I422_8:
      return VPX_IMG_FMT_I422;
    case SUPERI_VPX_FORMAT_I444_8:
      return VPX_IMG_FMT_I444;
    case SUPERI_VPX_FORMAT_I420_10:
      return VPX_IMG_FMT_I42016;
    case SUPERI_VPX_FORMAT_I422_10:
      return VPX_IMG_FMT_I42216;
    case SUPERI_VPX_FORMAT_I444_10:
      return VPX_IMG_FMT_I44416;
    default:
      return VPX_IMG_FMT_NONE;
  }
}

const char *superi_vpx_status_string(const struct superi_vpx_api *api,
                                    int status) {
  if (status == SUPERI_VPX_INVALID_ARGUMENT) return "invalid shim argument";
  if (status == SUPERI_VPX_OUT_OF_MEMORY) return "shim allocation failed";
  if (status == SUPERI_VPX_UNSUPPORTED_FORMAT) return "unsupported pixel format";
  if (status == SUPERI_VPX_BUFFER_MISMATCH) return "pixel buffer geometry mismatch";
  if (api == NULL || api->codec_err_to_string == NULL) return "unknown libvpx error";
  return ((codec_err_to_string_fn)api->codec_err_to_string)(
      (vpx_codec_err_t)status);
}

int superi_vpx_decoder_create(const struct superi_vpx_api *api, int codec,
                              uint32_t threads,
                              struct superi_vpx_decoder **decoder) {
  vpx_codec_dec_cfg_t config;
  vpx_codec_iface_t *interface;
  struct superi_vpx_decoder *created;
  vpx_codec_err_t status;
  if (api == NULL || decoder == NULL || threads == 0) {
    return SUPERI_VPX_INVALID_ARGUMENT;
  }
  *decoder = NULL;
  interface = decoder_interface(api, codec);
  if (interface == NULL) return SUPERI_VPX_INVALID_ARGUMENT;
  created = (struct superi_vpx_decoder *)calloc(1, sizeof(*created));
  if (created == NULL) return SUPERI_VPX_OUT_OF_MEMORY;
  created->api = api;
  config.threads = threads;
  config.w = 0;
  config.h = 0;
  status = ((decoder_init_fn)api->codec_dec_init_ver)(
      &created->context, interface, &config, 0, VPX_DECODER_ABI_VERSION);
  if (status != VPX_CODEC_OK) {
    free(created);
    return (int)status;
  }
  created->initialized = 1;
  *decoder = created;
  return VPX_CODEC_OK;
}

int superi_vpx_decoder_decode(struct superi_vpx_decoder *decoder,
                              const uint8_t *data, size_t size) {
  vpx_codec_err_t status;
  if (decoder == NULL || (data == NULL && size != 0) || size > UINT_MAX) {
    return SUPERI_VPX_INVALID_ARGUMENT;
  }
  decoder->iterator = NULL;
  decoder->current = NULL;
  status = ((decode_fn)decoder->api->codec_decode)(
      &decoder->context, data, (unsigned int)size, NULL, 0);
  return (int)status;
}

int superi_vpx_decoder_next(struct superi_vpx_decoder *decoder,
                            struct superi_vpx_frame_info *frame) {
  int format;
  if (decoder == NULL || frame == NULL) return SUPERI_VPX_INVALID_ARGUMENT;
  decoder->current = ((get_frame_fn)decoder->api->codec_get_frame)(
      &decoder->context, &decoder->iterator);
  if (decoder->current == NULL) return 0;
  format = image_format(decoder->current->fmt, decoder->current->bit_depth);
  if (format < 0) return format;
  frame->width = decoder->current->d_w;
  frame->height = decoder->current->d_h;
  frame->format = format;
  frame->bit_depth = decoder->current->bit_depth;
  frame->color_space = (int32_t)decoder->current->cs;
  frame->color_range = (int32_t)decoder->current->range;
  return 1;
}

int superi_vpx_decoder_copy_plane(const struct superi_vpx_decoder *decoder,
                                  uint32_t plane, uint8_t *destination,
                                  size_t destination_stride,
                                  uint32_t destination_rows,
                                  size_t destination_row_bytes) {
  uint32_t width;
  uint32_t rows;
  size_t bytes_per_sample;
  size_t expected_row_bytes;
  uint32_t row;
  const uint8_t *source;
  ptrdiff_t source_stride;
  if (decoder == NULL || decoder->current == NULL || destination == NULL ||
      plane > 2) {
    return SUPERI_VPX_INVALID_ARGUMENT;
  }
  width = decoder->current->d_w;
  rows = decoder->current->d_h;
  if (plane > 0) {
    const uint32_t x_mask = (1u << decoder->current->x_chroma_shift) - 1u;
    const uint32_t y_mask = (1u << decoder->current->y_chroma_shift) - 1u;
    width = (width >> decoder->current->x_chroma_shift) +
            ((width & x_mask) != 0u);
    rows = (rows >> decoder->current->y_chroma_shift) +
           ((rows & y_mask) != 0u);
  }
  bytes_per_sample =
      (decoder->current->fmt & VPX_IMG_FMT_HIGHBITDEPTH) != 0 ? 2u : 1u;
  expected_row_bytes = (size_t)width * bytes_per_sample;
  if (destination_rows != rows || destination_row_bytes != expected_row_bytes ||
      destination_stride < expected_row_bytes) {
    return SUPERI_VPX_BUFFER_MISMATCH;
  }
  source = decoder->current->planes[plane];
  source_stride = decoder->current->stride[plane];
  if (source == NULL || source_stride == 0) return SUPERI_VPX_BUFFER_MISMATCH;
  for (row = 0; row < rows; ++row) {
    memcpy(destination + (size_t)row * destination_stride,
           source + (ptrdiff_t)row * source_stride, expected_row_bytes);
  }
  return VPX_CODEC_OK;
}

const char *superi_vpx_decoder_error(
    const struct superi_vpx_decoder *decoder) {
  if (decoder == NULL || decoder->api->codec_error == NULL) {
    return "decoder is unavailable";
  }
  return ((codec_error_fn)decoder->api->codec_error)(&decoder->context);
}

void superi_vpx_decoder_destroy(struct superi_vpx_decoder *decoder) {
  if (decoder == NULL) return;
  if (decoder->initialized) {
    ((destroy_fn)decoder->api->codec_destroy)(&decoder->context);
  }
  free(decoder);
}

int superi_vpx_encoder_create(const struct superi_vpx_api *api, int codec,
                              int format, uint32_t width, uint32_t height,
                              uint32_t timebase_numerator,
                              uint32_t timebase_denominator,
                              uint32_t target_bitrate_kbps, uint32_t threads,
                              struct superi_vpx_encoder **encoder) {
  vpx_codec_iface_t *interface;
  vpx_codec_enc_cfg_t config;
  vpx_codec_flags_t flags = 0;
  struct superi_vpx_encoder *created;
  vpx_codec_err_t status;
  const int high_bit_depth = format >= SUPERI_VPX_FORMAT_I420_10;
  if (api == NULL || encoder == NULL || width == 0 || height == 0 ||
      timebase_numerator == 0 || timebase_denominator == 0 || threads == 0 ||
      native_image_format(format) == VPX_IMG_FMT_NONE) {
    return SUPERI_VPX_INVALID_ARGUMENT;
  }
  if (codec == SUPERI_VPX_CODEC_VP8 &&
      format != SUPERI_VPX_FORMAT_I420_8) {
    return SUPERI_VPX_UNSUPPORTED_FORMAT;
  }
  *encoder = NULL;
  interface = encoder_interface(api, codec);
  if (interface == NULL) return SUPERI_VPX_INVALID_ARGUMENT;
  status = ((encoder_config_default_fn)api->codec_enc_config_default)(
      interface, &config, 0);
  if (status != VPX_CODEC_OK) return (int)status;

  config.g_w = width;
  config.g_h = height;
  config.g_threads = threads;
  config.g_timebase.num = (int)timebase_numerator;
  config.g_timebase.den = (int)timebase_denominator;
  config.g_lag_in_frames = 0;
  config.g_pass = VPX_RC_ONE_PASS;
  config.rc_end_usage = VPX_VBR;
  config.rc_target_bitrate = target_bitrate_kbps;
  config.kf_mode = VPX_KF_AUTO;
  config.kf_min_dist = 0;
  config.kf_max_dist = 120;
  if (format == SUPERI_VPX_FORMAT_I420_8) {
    config.g_profile = 0;
  } else if (format == SUPERI_VPX_FORMAT_I422_8 ||
             format == SUPERI_VPX_FORMAT_I444_8) {
    config.g_profile = 1;
  } else if (format == SUPERI_VPX_FORMAT_I420_10) {
    config.g_profile = 2;
  } else {
    config.g_profile = 3;
  }
  config.g_bit_depth = high_bit_depth ? VPX_BITS_10 : VPX_BITS_8;
  config.g_input_bit_depth = high_bit_depth ? 10u : 8u;
  if (high_bit_depth) flags |= VPX_CODEC_USE_HIGHBITDEPTH;

  created = (struct superi_vpx_encoder *)calloc(1, sizeof(*created));
  if (created == NULL) return SUPERI_VPX_OUT_OF_MEMORY;
  created->api = api;
  created->codec = codec;
  status = ((encoder_init_fn)api->codec_enc_init_ver)(
      &created->context, interface, &config, flags, VPX_ENCODER_ABI_VERSION);
  if (status != VPX_CODEC_OK) {
    free(created);
    return (int)status;
  }
  created->initialized = 1;
  *encoder = created;
  return VPX_CODEC_OK;
}

static int plane_size(uint32_t width, uint32_t height,
                      size_t bytes_per_sample, size_t *size) {
  size_t samples;
  if (size == NULL || (size_t)width > SIZE_MAX / (size_t)height) {
    return SUPERI_VPX_BUFFER_MISMATCH;
  }
  samples = (size_t)width * (size_t)height;
  if (samples > SIZE_MAX / bytes_per_sample) {
    return SUPERI_VPX_BUFFER_MISMATCH;
  }
  *size = samples * bytes_per_sample;
  return VPX_CODEC_OK;
}

int superi_vpx_encoder_encode(struct superi_vpx_encoder *encoder,
                              const uint8_t *data, size_t data_size, int format,
                              uint32_t width, uint32_t height, int64_t pts,
                              uint64_t duration, int force_keyframe,
                              int color_space, int color_range) {
  vpx_image_t *image;
  vpx_img_fmt_t image_format;
  vpx_enc_frame_flags_t flags = 0;
  vpx_codec_err_t status;
  uint32_t chroma_width;
  uint32_t chroma_height;
  uint32_t row;
  size_t bytes_per_sample;
  size_t luma_size;
  size_t chroma_size;
  size_t expected_size;
  const uint8_t *source;
  if (encoder == NULL || data == NULL || width == 0 || height == 0 ||
      duration == 0 || duration > ULONG_MAX) {
    return SUPERI_VPX_INVALID_ARGUMENT;
  }
  image_format = native_image_format(format);
  if (image_format == VPX_IMG_FMT_NONE) return SUPERI_VPX_UNSUPPORTED_FORMAT;
  bytes_per_sample = format >= SUPERI_VPX_FORMAT_I420_10 ? 2u : 1u;
  chroma_width = format == SUPERI_VPX_FORMAT_I444_8 ||
                         format == SUPERI_VPX_FORMAT_I444_10
                     ? width
                     : width / 2u + width % 2u;
  chroma_height = format == SUPERI_VPX_FORMAT_I420_8 ||
                          format == SUPERI_VPX_FORMAT_I420_10
                      ? height / 2u + height % 2u
                      : height;
  if (plane_size(width, height, bytes_per_sample, &luma_size) !=
          VPX_CODEC_OK ||
      plane_size(chroma_width, chroma_height, bytes_per_sample,
                 &chroma_size) != VPX_CODEC_OK ||
      chroma_size > (SIZE_MAX - luma_size) / 2u) {
    return SUPERI_VPX_BUFFER_MISMATCH;
  }
  expected_size = luma_size + chroma_size * 2u;
  if (data_size != expected_size) return SUPERI_VPX_BUFFER_MISMATCH;

  image = ((image_alloc_fn)encoder->api->img_alloc)(NULL, image_format, width,
                                                    height, 1);
  if (image == NULL) return SUPERI_VPX_OUT_OF_MEMORY;
  image->bit_depth = bytes_per_sample == 2u ? 10u : 8u;
  source = data;
  for (row = 0; row < height; ++row) {
    memcpy(image->planes[VPX_PLANE_Y] + (size_t)row * image->stride[VPX_PLANE_Y],
           source + (size_t)row * width * bytes_per_sample,
           (size_t)width * bytes_per_sample);
  }
  source += luma_size;
  for (row = 0; row < chroma_height; ++row) {
    memcpy(image->planes[VPX_PLANE_U] + (size_t)row * image->stride[VPX_PLANE_U],
           source + (size_t)row * chroma_width * bytes_per_sample,
           (size_t)chroma_width * bytes_per_sample);
  }
  source += chroma_size;
  for (row = 0; row < chroma_height; ++row) {
    memcpy(image->planes[VPX_PLANE_V] + (size_t)row * image->stride[VPX_PLANE_V],
           source + (size_t)row * chroma_width * bytes_per_sample,
           (size_t)chroma_width * bytes_per_sample);
  }
  image->cs = (vpx_color_space_t)color_space;
  image->range = (vpx_color_range_t)color_range;
  if (encoder->codec == SUPERI_VPX_CODEC_VP9) {
    status = ((codec_control_fn)encoder->api->codec_control)(
        &encoder->context, VP9E_SET_COLOR_SPACE, color_space);
    if (status != VPX_CODEC_OK) {
      ((image_free_fn)encoder->api->img_free)(image);
      return (int)status;
    }
    status = ((codec_control_fn)encoder->api->codec_control)(
        &encoder->context, VP9E_SET_COLOR_RANGE, color_range);
    if (status != VPX_CODEC_OK) {
      ((image_free_fn)encoder->api->img_free)(image);
      return (int)status;
    }
  }
  if (force_keyframe) flags |= VPX_EFLAG_FORCE_KF;
  encoder->iterator = NULL;
  status = ((encode_fn)encoder->api->codec_encode)(
      &encoder->context, image, (vpx_codec_pts_t)pts,
      (unsigned long)duration, flags, VPX_DL_GOOD_QUALITY);
  ((image_free_fn)encoder->api->img_free)(image);
  return (int)status;
}

int superi_vpx_encoder_flush(struct superi_vpx_encoder *encoder) {
  vpx_codec_err_t status;
  if (encoder == NULL) return SUPERI_VPX_INVALID_ARGUMENT;
  encoder->iterator = NULL;
  status = ((encode_fn)encoder->api->codec_encode)(
      &encoder->context, NULL, 0, 0, 0, VPX_DL_GOOD_QUALITY);
  return (int)status;
}

int superi_vpx_encoder_next(struct superi_vpx_encoder *encoder,
                            struct superi_vpx_packet_info *packet) {
  const vpx_codec_cx_pkt_t *native;
  if (encoder == NULL || packet == NULL) return SUPERI_VPX_INVALID_ARGUMENT;
  for (;;) {
    native = ((get_packet_fn)encoder->api->codec_get_cx_data)(
        &encoder->context, &encoder->iterator);
    if (native == NULL) return 0;
    if (native->kind != VPX_CODEC_CX_FRAME_PKT) continue;
    packet->data = (const uint8_t *)native->data.frame.buf;
    packet->size = native->data.frame.sz;
    packet->pts = native->data.frame.pts;
    packet->duration = native->data.frame.duration;
    packet->flags = native->data.frame.flags;
    return 1;
  }
}

const char *superi_vpx_encoder_error(
    const struct superi_vpx_encoder *encoder) {
  if (encoder == NULL || encoder->api->codec_error == NULL) {
    return "encoder is unavailable";
  }
  return ((codec_error_fn)encoder->api->codec_error)(&encoder->context);
}

void superi_vpx_encoder_destroy(struct superi_vpx_encoder *encoder) {
  if (encoder == NULL) return;
  if (encoder->initialized) {
    ((destroy_fn)encoder->api->codec_destroy)(&encoder->context);
  }
  free(encoder);
}

#ifndef SUPERI_VPX_SHIM_H_
#define SUPERI_VPX_SHIM_H_

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

enum superi_vpx_codec {
  SUPERI_VPX_CODEC_VP8 = 8,
  SUPERI_VPX_CODEC_VP9 = 9,
};

enum superi_vpx_format {
  SUPERI_VPX_FORMAT_I420_8 = 1,
  SUPERI_VPX_FORMAT_I422_8 = 2,
  SUPERI_VPX_FORMAT_I444_8 = 3,
  SUPERI_VPX_FORMAT_I420_10 = 11,
  SUPERI_VPX_FORMAT_I422_10 = 12,
  SUPERI_VPX_FORMAT_I444_10 = 13,
};

struct superi_vpx_api {
  void *codec_err_to_string;
  void *codec_error;
  void *codec_control;
  void *codec_vp8_dx;
  void *codec_vp9_dx;
  void *codec_dec_init_ver;
  void *codec_decode;
  void *codec_get_frame;
  void *codec_vp8_cx;
  void *codec_vp9_cx;
  void *codec_enc_config_default;
  void *codec_enc_init_ver;
  void *codec_encode;
  void *codec_get_cx_data;
  void *codec_destroy;
  void *img_alloc;
  void *img_free;
};

struct superi_vpx_frame_info {
  uint32_t width;
  uint32_t height;
  int32_t format;
  uint32_t bit_depth;
  int32_t color_space;
  int32_t color_range;
};

struct superi_vpx_packet_info {
  const uint8_t *data;
  size_t size;
  int64_t pts;
  uint64_t duration;
  uint32_t flags;
};

struct superi_vpx_decoder;
struct superi_vpx_encoder;

const char *superi_vpx_status_string(const struct superi_vpx_api *api,
                                    int status);

int superi_vpx_decoder_create(const struct superi_vpx_api *api, int codec,
                              uint32_t threads,
                              struct superi_vpx_decoder **decoder);
int superi_vpx_decoder_decode(struct superi_vpx_decoder *decoder,
                              const uint8_t *data, size_t size);
int superi_vpx_decoder_next(struct superi_vpx_decoder *decoder,
                            struct superi_vpx_frame_info *frame);
int superi_vpx_decoder_copy_plane(const struct superi_vpx_decoder *decoder,
                                  uint32_t plane, uint8_t *destination,
                                  size_t destination_stride,
                                  uint32_t destination_rows,
                                  size_t destination_row_bytes);
const char *superi_vpx_decoder_error(
    const struct superi_vpx_decoder *decoder);
void superi_vpx_decoder_destroy(struct superi_vpx_decoder *decoder);

int superi_vpx_encoder_create(const struct superi_vpx_api *api, int codec,
                              int format, uint32_t width, uint32_t height,
                              uint32_t timebase_numerator,
                              uint32_t timebase_denominator,
                              uint32_t target_bitrate_kbps, uint32_t threads,
                              struct superi_vpx_encoder **encoder);
int superi_vpx_encoder_encode(struct superi_vpx_encoder *encoder,
                              const uint8_t *data, size_t data_size, int format,
                              uint32_t width, uint32_t height, int64_t pts,
                              uint64_t duration, int force_keyframe,
                              int color_space, int color_range);
int superi_vpx_encoder_flush(struct superi_vpx_encoder *encoder);
int superi_vpx_encoder_next(struct superi_vpx_encoder *encoder,
                            struct superi_vpx_packet_info *packet);
const char *superi_vpx_encoder_error(
    const struct superi_vpx_encoder *encoder);
void superi_vpx_encoder_destroy(struct superi_vpx_encoder *encoder);

#ifdef __cplusplus
}
#endif

#endif

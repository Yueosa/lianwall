#include "video_gpu.h"
#include <libavutil/hwcontext_vulkan.h>
#include <libavutil/pixdesc.h>
#include <libplacebo/utils/libav.h>
#include <libplacebo/vulkan.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <vulkan/vulkan.h>

struct lw_video_gpu {
    pl_gpu gpu;
    pl_tex textures[4];
    struct pl_frame frame;
    double sar;
    bool mapped;
    bool hardware;
    char error[256];
};
static int error(struct lw_video_gpu *m, const char *message) {
    snprintf(m->error, sizeof(m->error), "%s", message);
    return -1;
}
struct lw_video_gpu *lw_video_gpu_create(pl_gpu gpu) {
    if (!gpu)
        return NULL;
    struct lw_video_gpu *m = calloc(1, sizeof(*m));
    if (m) {
        m->gpu = gpu;
        m->sar = 1;
    }
    return m;
}
void lw_video_gpu_clear(struct lw_video_gpu *m) {
    if (!m)
        return;
    if (m->mapped)
        pl_unmap_avframe(m->gpu, &m->frame);
    m->frame = (struct pl_frame){0};
    m->mapped = false;
}
struct lw_video_gpu *lw_video_gpu_create_hardware(pl_gpu gpu) {
    struct lw_video_gpu *m = lw_video_gpu_create(gpu);
    if (m)
        m->hardware = true;
    return m;
}
int lw_video_gpu_map(struct lw_video_gpu *m, const AVFrame *frame) {
    if (!m || !frame)
        return -1;
    const AVPixFmtDescriptor *format = av_pix_fmt_desc_get(frame->format);
    if (!format)
        return error(m, "Unknown video pixel format");
    if ((format->flags & AV_PIX_FMT_FLAG_HWACCEL) || frame->hw_frames_ctx) {
        if (!m->hardware || frame->format != AV_PIX_FMT_VULKAN || !frame->hw_frames_ctx)
            return error(m, "Vulkan frame mapping was not enabled for this source");
        AVHWFramesContext *pool = (void *)frame->hw_frames_ctx->data;
        if (!pool->device_ctx || pool->device_ctx->type != AV_HWDEVICE_TYPE_VULKAN)
            return error(m, "Video hardware context is not Vulkan");
        pl_vulkan vk = pl_vulkan_get(m->gpu);
        AVVulkanDeviceContext *device = pool->device_ctx->hwctx;
        if (!vk || device->act_dev != vk->device)
            return error(m, "Video frame belongs to another Vulkan device");
    }
    if (frame->width < 1 || frame->height < 1 || frame->width > 8192 || frame->height > 8192 ||
        (int64_t)frame->width * frame->height > 16 * 1024 * 1024)
        return error(m, "Video frame exceeds mapping budget");
    if (frame->color_trc == AVCOL_TRC_SMPTE2084 || frame->color_trc == AVCOL_TRC_ARIB_STD_B67 ||
        av_frame_get_side_data(frame, AV_FRAME_DATA_DOVI_METADATA))
        return error(m, "HDR video rendering is not enabled");
    AVFrameSideData *matrix = av_frame_get_side_data(frame, AV_FRAME_DATA_DISPLAYMATRIX);
    if (matrix) {
        if (matrix->size < 9 * sizeof(int32_t))
            return error(m, "Invalid display matrix");
        const int32_t *values = (const int32_t *)matrix->data;
        int a = values[0] / 65536, b = values[1] / 65536, c = values[3] / 65536,
            d = values[4] / 65536;
        if (values[0] != a * 65536 || values[1] != b * 65536 || values[3] != c * 65536 ||
            values[4] != d * 65536 || values[2] || values[5] || values[8] != (1 << 30) ||
            abs(a) + abs(c) != 1 || abs(b) + abs(d) != 1 || abs(a) + abs(b) != 1 ||
            abs(c) + abs(d) != 1 || a * d - b * c != 1)
            return error(m, "Mirrored or non-orthogonal video transform is not supported yet");
    }
    double sar = frame->sample_aspect_ratio.num > 0 && frame->sample_aspect_ratio.den > 0
                     ? av_q2d(frame->sample_aspect_ratio)
                     : 1;
    if (!isfinite(sar) || sar < 1.0 / 64 || sar > 64)
        return error(m, "Invalid sample aspect ratio");
    lw_video_gpu_clear(m);
    if (!pl_map_avframe_ex(
            m->gpu, &m->frame,
            pl_avframe_params(.frame = frame, .tex = m->textures, .map_dovi = false)))
        return error(m, "Failed to upload video frame planes");
    m->mapped = true;
    m->sar = sar;
    return 0;
}
const struct pl_frame *lw_video_gpu_frame(const struct lw_video_gpu *m) {
    return m && m->mapped ? &m->frame : NULL;
}
double lw_video_gpu_aspect(const struct lw_video_gpu *m) {
    return m && m->mapped
               ? pl_aspect_rotate(pl_rect2df_aspect(&m->frame.crop) * m->sar, m->frame.rotation)
               : 0;
}
int lw_video_gpu_render(struct lw_video_gpu *m, pl_renderer renderer, const struct pl_frame *target,
                        bool fit) {
    if (!m || !m->mapped || !renderer || !target)
        return -1;
    struct pl_frame source = m->frame, destination = *target;
    int rotation = pl_rotation_normalize(source.rotation - destination.rotation);
    double aspect = pl_aspect_rotate(pl_rect2df_aspect(&source.crop) * m->sar, rotation);
    if (fit)
        pl_rect2df_aspect_set(&destination.crop, aspect, 0);
    else {
        double required = pl_aspect_rotate(pl_rect2df_aspect(&destination.crop), rotation) / m->sar;
        pl_rect2df_aspect_set(&source.crop, required, 0);
    }
    struct pl_render_params params = pl_render_default_params;
    params.border = PL_CLEAR_COLOR;
    params.background = PL_CLEAR_COLOR;
    params.background_transparency = 0;
    return pl_render_image(renderer, &source, &destination, &params)
               ? 0
               : error(m, "Failed to render mapped video");
}
const char *lw_video_gpu_error(const struct lw_video_gpu *m) {
    return m ? m->error : "Video GPU allocation failed";
}
void lw_video_gpu_destroy(struct lw_video_gpu *m) {
    if (!m)
        return;
    lw_video_gpu_clear(m);
    for (int i = 0; i < 4; i++)
        pl_tex_destroy(m->gpu, &m->textures[i]);
    free(m);
}

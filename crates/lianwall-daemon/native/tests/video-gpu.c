#include "video.h"
#include "video_gpu.h"
#include <libavutil/display.h>
#include <libavutil/pixdesc.h>
#include <libplacebo/log.h>
#include <libplacebo/vulkan.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <vulkan/vulkan.h>
static int errors;
static void logger(void *data, enum pl_log_level level, const char *message) {
    (void)data;
    if (level <= PL_LOG_ERR) {
        errors++;
        fprintf(stderr, "GPU: %s\n", message);
    }
}
static struct pl_frame target_frame(pl_tex tex) {
    return (struct pl_frame){
        .num_planes = 1,
        .planes = {{.texture = tex, .components = 4, .component_mapping = {0, 1, 2, 3}}},
        .repr = pl_color_repr_rgb,
        .color = pl_color_space_srgb,
        .crop = {0, 0, tex->params.w, tex->params.h}};
}
int main(int argc, char **argv) {
    if (argc != 3 && argc != 7)
        return 2;
    pl_log log =
        pl_log_create(PL_API_VER, pl_log_params(.log_cb = logger, .log_level = PL_LOG_WARN));
    pl_vk_inst instance = pl_vk_inst_create(
        log, pl_vk_inst_params(.debug = getenv("LIANWALL_TEST_VALIDATION") != NULL));
    if (!instance)
        return 3;
    if (getenv("LIANWALL_TEST_VALIDATION")) {
        int enabled = 0;
        for (int i = 0; i < instance->num_layers; i++)
            if (!strcmp(instance->layers[i], "VK_LAYER_KHRONOS_validation"))
                enabled = 1;
        if (!enabled)
            return 4;
        printf("VALIDATION_ENABLED\n");
    }
    pl_vulkan vk = pl_vulkan_create(
        log, pl_vulkan_params(.instance = instance->instance, .allow_software = false));
    if (!vk)
        return 5;
    pl_renderer renderer = pl_renderer_create(log, vk->gpu);
    struct lw_video_gpu *mapping = lw_video_gpu_create(vk->gpu);
    struct lw_video *video = lw_video_create(NULL, NULL);
    if (!renderer || !mapping || !video || lw_video_open(video, argv[2]))
        return 6;
    int w = argc == 7 ? atoi(argv[4]) : 128, h = argc == 7 ? atoi(argv[5]) : 72,
        fit = argc == 7 ? atoi(argv[6]) : 1;
    pl_tex target = pl_tex_create(
        vk->gpu, pl_tex_params(.w = w, .h = h, .format = pl_find_named_fmt(vk->gpu, "rgba8"),
                               .renderable = true, .blit_dst = true, .host_readable = true));
    if (!target)
        return 7;
    unsigned char *bytes = malloc((size_t)w * h * 4);
    if (!bytes)
        return 8;
    struct pl_frame dst = target_frame(target);
    struct lw_video_frame frame = {0};
    int result = 0, frames = 0;
    bool negative_checked = false;
    while ((result = lw_video_next(video, &frame)) == LW_VIDEO_FRAME) {
        if (lw_video_gpu_map(mapping, frame.frame)) {
            fprintf(stderr, "map: %s\n", lw_video_gpu_error(mapping));
            return 9;
        }
        const struct pl_frame *source = lw_video_gpu_frame(mapping);
        if (!source || source->num_planes < 1 || lw_video_gpu_aspect(mapping) <= 0)
            return 10;
        if (!negative_checked) {
            pl_tex first = source->planes[0].texture;
            AVFrame *bad = av_frame_clone(frame.frame);
            if (!bad)
                return 11;
            bad->color_trc = AVCOL_TRC_SMPTE2084;
            if (lw_video_gpu_map(mapping, bad) != -1 ||
                lw_video_gpu_frame(mapping)->planes[0].texture != first)
                return 12;
            bad->color_trc = frame.frame->color_trc;
            bad->format = AV_PIX_FMT_VULKAN;
            if (lw_video_gpu_map(mapping, bad) != -1)
                return 13;
            bad->format = frame.frame->format;
            av_frame_remove_side_data(bad, AV_FRAME_DATA_DISPLAYMATRIX);
            AVFrameSideData *matrix =
                av_frame_new_side_data(bad, AV_FRAME_DATA_DISPLAYMATRIX, 9 * sizeof(int32_t));
            if (!matrix)
                return 14;
            av_display_rotation_set((int32_t *)matrix->data, 0);
            av_display_matrix_flip((int32_t *)matrix->data, 1, 0);
            if (lw_video_gpu_map(mapping, bad) != -1 ||
                lw_video_gpu_frame(mapping)->planes[0].texture != first)
                return 15;
            av_frame_free(&bad);
            negative_checked = true;
        }
        // The mapped GPU frame must own its AVFrame/metadata independently.
        lw_video_frame_clear(&frame);
        if (lw_video_gpu_render(mapping, renderer, &dst, fit) ||
            !pl_tex_download(vk->gpu, pl_tex_transfer_params(.tex = target, .ptr = bytes)))
            return 16;
        frames++;
        if (argc == 7)
            break;
    }
    if (argc == 3 && result != LW_VIDEO_EOF) {
        fprintf(stderr, "decode: %s\n", lw_video_error(video));
        return 17;
    }
    lw_video_frame_clear(&frame);
    lw_video_destroy(video);
    // Rendering remains valid after both original AVFrame and decoder are gone.
    if (lw_video_gpu_render(mapping, renderer, &dst, fit) ||
        !pl_tex_download(vk->gpu, pl_tex_transfer_params(.tex = target, .ptr = bytes)))
        return 18;
    if (argc == 7) {
        FILE *file = fopen(argv[3], "wb");
        if (!file || fwrite(bytes, 4, (size_t)w * h, file) != (size_t)w * h)
            return 19;
        fclose(file);
    }
    printf("GPU_VIDEO frames=%d output=%dx%d aspect=%.9f rotation=%d errors=%d\n", frames, w, h,
           lw_video_gpu_aspect(mapping), lw_video_gpu_frame(mapping)->rotation, errors);
    free(bytes);
    lw_video_gpu_destroy(mapping);
    pl_tex_destroy(vk->gpu, &target);
    pl_renderer_destroy(&renderer);
    pl_gpu_finish(vk->gpu);
    pl_vulkan_destroy(&vk);
    pl_vk_inst_destroy(&instance);
    pl_log_destroy(&log);
    return frames && errors == 0 ? 0 : 20;
}

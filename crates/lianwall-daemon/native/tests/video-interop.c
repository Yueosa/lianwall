/* Offscreen shared-device investigation. No daemon/Wayland changes. */
#include "video.h"
#include "video_hw.h"
#include <libavcodec/avcodec.h>
#include <libavutil/hwcontext.h>
#include <libavutil/hwcontext_vulkan.h>
#include <libavutil/pixdesc.h>
#include <libplacebo/log.h>
#include <libplacebo/renderer.h>
#include <libplacebo/utils/libav.h>
#include <libplacebo/vulkan.h>
#include <math.h>
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#define REQUIRE(condition)                                                                         \
    do {                                                                                           \
        if (!(condition)) {                                                                        \
            fprintf(stderr, "FAIL line %d: %s\n", __LINE__, #condition);                           \
            exit(1);                                                                               \
        }                                                                                          \
    } while (0)
static unsigned errors;
static void logger(void *opaque, enum pl_log_level level, const char *message) {
    (void)opaque;
    if (level <= PL_LOG_ERR)
        errors++;
    fprintf(stderr, "GPU[%d]: %s\n", level, message);
}
int main(int argc, char **argv) {
    if (argc != 2)
        return 2;
    pl_log log =
        pl_log_create(PL_API_VER, pl_log_params(.log_cb = logger, .log_level = PL_LOG_INFO));
    const char *layers[] = {"VK_LAYER_KHRONOS_validation"};
    pl_vk_inst instance = pl_vk_inst_create(
        log, pl_vk_inst_params(.layers = layers,
                               .num_layers = getenv("LIANWALL_TEST_VALIDATION") ? 1 : 0));
    REQUIRE(instance);
    if (getenv("LIANWALL_TEST_VALIDATION")) {
        bool enabled = false;
        for (int i = 0; i < instance->num_layers; i++)
            if (!strcmp(instance->layers[i], "VK_LAYER_KHRONOS_validation"))
                enabled = true;
        REQUIRE(enabled);
    }
    pl_vulkan vk = lw_video_device_create(log, instance->instance, VK_NULL_HANDLE);
    REQUIRE(vk);
    AVBufferRef *device = lw_video_hw_create(vk, instance);
    REQUIRE(device);
    struct lw_video *decoder = lw_video_create(NULL, NULL);
    REQUIRE(decoder);
    REQUIRE(!lw_video_set_hardware(decoder, device, lw_video_hw_check, device->data));
    REQUIRE(!lw_video_set_hardware_frames(decoder, lw_video_hw_prepare));
    if (lw_video_open(decoder, argv[1])) {
        fprintf(stderr, "OPEN: %s\n", lw_video_error(decoder));
        return 1;
    }
    struct lw_video *reference = lw_video_create(NULL, NULL);
    REQUIRE(reference);
    REQUIRE(!lw_video_open(reference, argv[1]));
    pl_renderer renderer = pl_renderer_create(log, vk->gpu);
    REQUIRE(renderer);
    pl_tex target = pl_tex_create(
        vk->gpu, pl_tex_params(.w = 128, .h = 72, .format = pl_find_named_fmt(vk->gpu, "rgba8"),
                               .renderable = true, .host_readable = true));
    REQUIRE(target);
    struct pl_frame destination = {
        .num_planes = 1,
        .planes = {{.texture = target, .components = 4, .component_mapping = {0, 1, 2, 3}}},
        .repr = pl_color_repr_rgb,
        .color = pl_color_space_srgb,
        .crop = {0, 0, 128, 72}};
    struct lw_video_frame frame = {0};
    struct pl_frame mapped = {0};
    bool have_mapping = false;
    unsigned frames = 0, max_delta = 0;
    int result;
    bool unsupported = false;
    unsigned char pixels[128 * 72 * 4], expected[128 * 72 * 4];
    pl_tex software_textures[4] = {0};
    for (unsigned loop = 0; loop < 3; loop++) {
        while ((result = lw_video_next(decoder, &frame)) == LW_VIDEO_FRAME) {
            REQUIRE(frame.frame->format == AV_PIX_FMT_VULKAN && frame.frame->hw_frames_ctx);
            AVHWFramesContext *frames_context = (void *)frame.frame->hw_frames_ctx->data;
            AVVulkanDeviceContext *frame_device = frames_context->device_ctx->hwctx;
            REQUIRE(frame_device->act_dev == vk->device);
            if (have_mapping)
                pl_unmap_avframe(vk->gpu, &mapped);
            REQUIRE(pl_map_avframe_ex(vk->gpu, &mapped,
                                      pl_avframe_params(.frame = frame.frame, .map_dovi = false)));
            uint64_t hardware_index = frame.index, hardware_loop = frame.loop;
            double hardware_pts = frame.pts;
            have_mapping = true;
            lw_video_frame_clear(&frame);
            REQUIRE(pl_render_image(renderer, &mapped, &destination, &pl_render_default_params));
            /* Test readback is final rendered RGBA, never decoder-frame hwdownload. */
            REQUIRE(pl_tex_download(vk->gpu, pl_tex_transfer_params(.tex = target, .ptr = pixels)));
            struct lw_video_frame software = {0};
            REQUIRE(lw_video_next(reference, &software) == LW_VIDEO_FRAME);
            REQUIRE(software.loop == hardware_loop && software.index == hardware_index &&
                    fabs(software.pts - hardware_pts) < 1e-8);
            struct pl_frame software_mapping = {0};
            REQUIRE(
                pl_map_avframe_ex(vk->gpu, &software_mapping,
                                  pl_avframe_params(.frame = software.frame,
                                                    .tex = software_textures, .map_dovi = false)));
            lw_video_frame_clear(&software);
            REQUIRE(pl_render_image(renderer, &software_mapping, &destination,
                                    &pl_render_default_params));
            REQUIRE(
                pl_tex_download(vk->gpu, pl_tex_transfer_params(.tex = target, .ptr = expected)));
            pl_unmap_avframe(vk->gpu, &software_mapping);
            for (unsigned i = 0; i < sizeof(pixels); i++) {
                unsigned delta = abs((int)pixels[i] - (int)expected[i]);
                if (delta > max_delta)
                    max_delta = delta;
            }
            frames++;
        }
        if (result != LW_VIDEO_EOF)
            fprintf(stderr, "DECODE: %s\n", lw_video_error(decoder));
        if (result == LW_VIDEO_ERROR && frames == 0 && errors == 0) {
            unsupported = true;
            goto cleanup;
        }
        REQUIRE(result == LW_VIDEO_EOF);
        struct lw_video_frame extra = {0};
        REQUIRE(lw_video_next(reference, &extra) == LW_VIDEO_EOF);
        if (loop < 2) {
            REQUIRE(!lw_video_rewind(decoder));
            REQUIRE(!lw_video_rewind(reference));
        }
    }
    REQUIRE(frames > 0 && have_mapping);
cleanup:
    lw_video_frame_clear(&frame);
    lw_video_destroy(decoder);
    lw_video_destroy(reference);
    if (have_mapping)
        REQUIRE(pl_render_image(renderer, &mapped, &destination, &pl_render_default_params));
    pl_gpu_finish(vk->gpu);
    if (have_mapping)
        pl_unmap_avframe(vk->gpu, &mapped);
    for (unsigned i = 0; i < 4; i++)
        pl_tex_destroy(vk->gpu, &software_textures[i]);
    pl_tex_destroy(vk->gpu, &target);
    pl_renderer_destroy(&renderer);
    av_buffer_unref(&device);
    pl_vulkan_destroy(&vk);
    pl_vk_inst_destroy(&instance);
    pl_log_destroy(&log);
    if (unsupported) {
        printf("INTEROP_UNSUPPORTED frames=0 errors=0\n");
        fflush(stdout);
        return 77;
    }
    printf("INTEROP frames=%u loops=3 decoder_downloads=0 rendered_readbacks=%u "
           "max_channel_delta=%u errors=%u\n",
           frames, frames * 2, max_delta, errors);
    fflush(stdout);
    return errors || max_delta > 4 ? 1 : 0;
}

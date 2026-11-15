#define _GNU_SOURCE
#include "video_gpu.h"
#include "video_hw.h"
#include "video_queue.h"
#include <dirent.h>
#include <poll.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#define CHECK(x)                                                                                   \
    do {                                                                                           \
        if (!(x)) {                                                                                \
            fprintf(stderr, "FAIL line %d: %s\n", __LINE__, #x);                                   \
            exit(1);                                                                               \
        }                                                                                          \
    } while (0)
static unsigned errors;
static void logger(void *unused, enum pl_log_level level, const char *text) {
    (void)unused;
    if (level <= PL_LOG_ERR) {
        errors++;
        fprintf(stderr, "GPU: %s\n", text);
    }
}
static double now(void) {
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return t.tv_sec + t.tv_nsec / 1e9;
}
static void sleep_ms(unsigned ms) {
    struct timespec t = {0, ms * 1000000};
    nanosleep(&t, NULL);
}
static int next(struct lw_video_queue *q, struct lw_video_frame *f, double *pts) {
    double end = now() + 8;
    for (;;) {
        int r = lw_video_queue_pop(q, f, pts);
        if (r != LW_VIDEO_AGAIN)
            return r;
        CHECK(now() < end);
        struct pollfd p = {lw_video_queue_fd(q), POLLIN, 0};
        CHECK(poll(&p, 1, 100) >= 0);
    }
}
static int fds(void) {
    DIR *d = opendir("/proc/self/fd");
    CHECK(d);
    int n = 0;
    struct dirent *e;
    while ((e = readdir(d)))
        if (e->d_name[0] != '.')
            n++;
    closedir(d);
    return n;
}
static struct lw_video_queue *create(const char *path, AVBufferRef *device, int mode, size_t bytes,
                                     uint64_t loops) {
    struct lw_video_queue_hardware hw = {.device = device,
                                         .check = lw_video_hw_check,
                                         .prepare = lw_video_hw_prepare,
                                         .opaque = device->data,
                                         .frame_bytes = lw_video_hw_bytes,
                                         .fallback = mode == 2};
    struct lw_video_queue *q =
        lw_video_queue_create_hardware(path, (struct lw_video_queue_options){4, bytes, loops}, hw);
    CHECK(q);
    return q;
}
static void full(struct lw_video_queue *q) {
    double end = now() + 8;
    struct lw_video_queue_stats s;
    do {
        lw_video_queue_stats(q, &s);
        if (s.waits && s.frames)
            return;
        CHECK(now() < end);
        sleep_ms(1);
    } while (1);
}
int main(int argc, char **argv) {
    if (argc != 4)
        return 2;
    int mode = atoi(argv[2]);
    size_t bytes = atoi(argv[3]) == 1 ? 1 : 64 * 1024 * 1024;
    pl_log log =
        pl_log_create(PL_API_VER, pl_log_params(.log_cb = logger, .log_level = PL_LOG_WARN));
    const char *layers[] = {"VK_LAYER_KHRONOS_validation"};
    pl_vk_inst instance = pl_vk_inst_create(
        log, pl_vk_inst_params(.layers = layers,
                               .num_layers = getenv("LIANWALL_TEST_VALIDATION") ? 1 : 0));
    CHECK(instance);
    if (getenv("LIANWALL_TEST_VALIDATION")) {
        bool enabled = false;
        for (int i = 0; i < instance->num_layers; i++)
            if (!strcmp(instance->layers[i], "VK_LAYER_KHRONOS_validation"))
                enabled = true;
        CHECK(enabled);
    }
    pl_vulkan vk = lw_video_device_create(log, instance->instance, VK_NULL_HANDLE);
    CHECK(vk);
    AVBufferRef *device = lw_video_hw_create(vk, instance);
    CHECK(device);
    struct lw_video_queue *q = create(argv[1], device, mode, bytes, 3);
    pl_renderer renderer = pl_renderer_create(log, vk->gpu);
    CHECK(renderer);
    struct lw_video_gpu *mapping = lw_video_gpu_create_hardware(vk->gpu);
    CHECK(mapping);
    pl_tex target = pl_tex_create(
        vk->gpu, pl_tex_params(.w = 128, .h = 72, .format = pl_find_named_fmt(vk->gpu, "rgba8"),
                               .renderable = true, .host_readable = true));
    CHECK(target);
    struct pl_frame dst = {
        .num_planes = 1,
        .planes = {{.texture = target, .components = 4, .component_mapping = {0, 1, 2, 3}}},
        .repr = pl_color_repr_rgb,
        .color = pl_color_space_srgb,
        .crop = {0, 0, 128, 72}};
    struct lw_video_frame frame = {0};
    double pts = 0, last = -1;
    unsigned count = 0;
    int result;
    unsigned char pixels[128 * 72 * 4];
    while ((result = next(q, &frame, &pts)) == LW_VIDEO_FRAME) {
        CHECK(pts > last);
        last = pts;
        CHECK(!lw_video_gpu_map(mapping, frame.frame));
        lw_video_frame_clear(&frame);
        CHECK(!lw_video_gpu_render(mapping, renderer, &dst, true));
        CHECK(pl_tex_download(vk->gpu, pl_tex_transfer_params(.tex = target, .ptr = pixels)));
        count++;
        if (count % 4 == 0)
            sleep_ms(2);
    }
    struct lw_video_queue_stats stats;
    lw_video_queue_stats(q, &stats);
    char reason[512];
    lw_video_queue_diagnostic(q, reason, sizeof(reason));
    if (mode == 3 || bytes == 1) {
        CHECK(result == LW_VIDEO_ERROR && count == 0);
    } else {
        CHECK(result == LW_VIDEO_EOF && count > 0);
        if (mode == 2) {
            CHECK(stats.fallbacks == 1 && stats.hardware_frames == 0 &&
                  stats.software_frames == count && *reason);
        } else
            CHECK(stats.hardware_frames == count && stats.fallbacks == 0);
    }
    CHECK(stats.peak_frames <= 4 && stats.peak_bytes <= bytes);
    lw_video_queue_destroy(q);
    if (count)
        CHECK(!lw_video_gpu_render(mapping, renderer, &dst, true));
    pl_gpu_finish(vk->gpu);
    lw_video_gpu_destroy(mapping);
    pl_tex_destroy(vk->gpu, &target);
    pl_renderer_destroy(&renderer);
    unsigned cancel_count = 0;
    double latency = 0;
    if (mode == 1 && bytes > 1) {
        int before = fds();
        for (unsigned i = 0; i < 20; i++) {
            q = create(argv[1], device, 1, bytes, 0);
            if (i % 2 == 0)
                full(q);
            double start = now();
            lw_video_queue_cancel(q);
            CHECK(lw_video_queue_pop(q, &frame, &pts) == LW_VIDEO_CANCELLED);
            lw_video_queue_destroy(q);
            double elapsed = (now() - start) * 1000;
            if (elapsed > latency)
                latency = elapsed;
            cancel_count++;
        }
        CHECK(fds() == before);
    }
    CHECK(av_buffer_get_ref_count(device) == 1);
    av_buffer_unref(&device);
    pl_vulkan_destroy(&vk);
    pl_vk_inst_destroy(&instance);
    pl_log_destroy(&log);
    printf("HW_QUEUE frames=%u hardware=%lu software=%lu fallbacks=%lu peak_frames=%u "
           "peak_bytes=%zu cancels=%u max_cancel_ms=%.4f errors=%u\n",
           count, (unsigned long)stats.hardware_frames, (unsigned long)stats.software_frames,
           (unsigned long)stats.fallbacks, stats.peak_frames, stats.peak_bytes, cancel_count,
           latency, errors);
    if (*reason)
        printf("FALLBACK_REASON %s\n", reason);
    fflush(stdout);
    return errors ? 1 : 0;
}

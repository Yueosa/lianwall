#include "image.h"
#include "transition.h"
#include <libplacebo/log.h>
#include <libplacebo/vulkan.h>
static int errors;
static void logger(void *unused, enum pl_log_level level, const char *message) {
    (void)unused;
    if (level <= PL_LOG_ERR) {
        errors++;
        fprintf(stderr, "%s\n", message);
    }
}
int main(int argc, char **argv) {
    if (argc != 2 && argc != 3)
        return 2;
    const char *names[] = {"red.jpg",      "red.png",       "red.bmp",     "red.webp",
                           "red.tiff",     "red.tga",       "red.pnm",     "red.ppm",
                           "red.pgm",      "red.pbm",       "red.pam",     "red.farbfeld",
                           "animated.gif", "animated.webp", "animated.png"};
    for (unsigned i = 0; i < sizeof(names) / sizeof(*names); i++) {
        char path[1024];
        snprintf(path, sizeof(path), "%s/%s", argv[1], names[i]);
        struct image_data image = {0};
        if (!decode_image(path, &image)) {
            fprintf(stderr, "DECODE FAIL %s\n", names[i]);
            return 3;
        }
        if (image.w != 32 || image.h != 16)
            return 4;
        if (strstr(names[i], "animated") &&
            (image.rgba[0] < 250 || image.rgba[1] > 5 || image.rgba[2] > 5))
            return 5;
        if (!strcmp(names[i], "red.pgm")) {
            if (abs(image.rgba[0] - 54) > 1 || image.rgba[0] != image.rgba[1] ||
                image.rgba[1] != image.rgba[2])
                return 14;
        } else if (!strcmp(names[i], "red.pbm")) {
            if (image.rgba[0] || image.rgba[1] || image.rgba[2])
                return 15;
        } else if (image.rgba[0] < 250 || image.rgba[1] > 5 || image.rgba[2] > 5 ||
                   image.rgba[3] != 255)
            return 16;
        free(image.rgba);
        printf("FORMAT %s PASS\n", names[i]);
    }
    for (int i = 0; i < 2; i++) {
        char path[1024];
        snprintf(path, sizeof(path), "%s/%s", argv[1], i ? "hdr.png" : "bad.png");
        struct image_data rejected = {0};
        if (decode_image(path, &rejected) || rejected.rgba)
            return 11;
    }
    {
        char path[1024];
        struct image_data metadata = {0};
        snprintf(path, sizeof(path), "%s/orientation6.jpg", argv[1]);
        if (!decode_image(path, &metadata) || metadata.w != 80 || metadata.h != 120 ||
            metadata.rgba[0] > 5 || metadata.rgba[1] > 5 || metadata.rgba[2] < 250)
            return 12;
        free(metadata.rgba);
        metadata = (struct image_data){0};
        snprintf(path, sizeof(path), "%s/linear-icc.png", argv[1]);
        if (!decode_image(path, &metadata) || abs(metadata.rgba[0] - 188) > 2)
            return 13;
        free(metadata.rgba);
        printf("EXIF and ICC metadata PASS\n");
    }
    if (argc == 3 && !strcmp(argv[2], "--decode-only"))
        return 0;
    pl_log log =
        pl_log_create(PL_API_VER, pl_log_params(.log_cb = logger, .log_level = PL_LOG_ERR));
    pl_vulkan vk = pl_vulkan_create(log, pl_vulkan_params(.allow_software = false));
    if (!vk)
        return 6;
    pl_dispatch dispatch = pl_dispatch_create(log, vk->gpu);
    pl_fmt fmt = pl_find_named_fmt(vk->gpu, "rgba8");
    unsigned char red[4 * 16], blue[4 * 16], bytes[4 * 16];
    for (int i = 0; i < 16; i++) {
        memcpy(red + i * 4, (unsigned char[]){255, (i / 4) * 60, (i % 4) * 60, 255}, 4);
        memcpy(blue + i * 4, (unsigned char[]){(i / 4) * 60, (i % 4) * 60, 255, 255}, 4);
    }
    pl_tex a = pl_tex_create(vk->gpu, pl_tex_params(.w = 4, .h = 4, .format = fmt,
                                                    .sampleable = true, .initial_data = red)),
           b = pl_tex_create(vk->gpu, pl_tex_params(.w = 4, .h = 4, .format = fmt,
                                                    .sampleable = true, .initial_data = blue)),
           dst = pl_tex_create(vk->gpu, pl_tex_params(.w = 4, .h = 4, .format = fmt,
                                                      .renderable = true, .host_readable = true));
    if (!dispatch || !a || !b || !dst)
        return 7;
    for (int step = 0; step <= 4; step++) {
        float t = step / 4.0f;
        if (!crossfade(dispatch, a, b, dst, t) ||
            !pl_tex_download(vk->gpu, pl_tex_transfer_params(.tex = dst, .ptr = bytes)))
            return 8;
        for (int i = 0; i < 16; i++)
            for (int component = 0; component < 4; component++) {
                int expected =
                    lroundf(red[i * 4 + component] * (1 - t) + blue[i * 4 + component] * t);
                if (abs(bytes[i * 4 + component] - expected) > 1)
                    return 9;
            }
        printf("FADE %.2f PASS\n", t);
    }
    pl_tex_destroy(vk->gpu, &a);
    pl_tex_destroy(vk->gpu, &b);
    pl_tex_destroy(vk->gpu, &dst);
    pl_dispatch_destroy(&dispatch);
    pl_vulkan_destroy(&vk);
    pl_log_destroy(&log);
    printf("GPU_ERRORS %d\n", errors);
    return errors ? 10 : 0;
}

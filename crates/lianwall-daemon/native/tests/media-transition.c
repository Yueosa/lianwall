/* Opt-in integration probe: private output only. Includes the owner to inspect
 * GPU blend pixels, clocks and rollback ownership, beyond frame-count checks. */
#define _GNU_SOURCE
#include "../engine.c"
#include <assert.h>
#include <fcntl.h>
#include <pthread.h>
#include <unistd.h>
static void pump_for(struct lw_engine *e, double seconds) {
    double end = image_now() + seconds;
    while (image_now() < end)
        assert(lw_pump(e, 10) >= 0);
}
static unsigned char *read_texture(struct lw_engine *e, pl_tex texture) {
    size_t bytes = (size_t)texture->params.w * texture->params.h * 4;
    unsigned char *data = malloc(bytes);
    pl_tex readable =
        pl_tex_create(e->vk->gpu, pl_tex_params(.w = texture->params.w, .h = texture->params.h,
                                                .format = texture->params.format,
                                                .renderable = true, .host_readable = true));
    assert(readable);
    struct pl_frame source = rgb_frame(texture), target = rgb_frame(readable);
    struct pl_render_params params = render_params();
    assert(pl_render_image(e->renderer, &source, &target, &params));
    assert(data &&
           pl_tex_download(e->vk->gpu, pl_tex_transfer_params(.tex = readable, .ptr = data)));
    pl_tex_destroy(e->vk->gpu, &readable);
    return data;
}
static void check_blend(struct lw_engine *e, int image) {
    /* Compare the real shader output to both actual scaled media targets. */
    for (int i = 0; i < 3; i++) {
        float progress = i * .5f;
        assert(!render_media(e, -2, image, NULL, progress, 1));
        unsigned char *old = read_texture(e, e->video_scaled[0]);
        unsigned char *next = read_texture(e, e->images[image].scaled);
        if (i < 2) {
            unsigned char *mixed = read_texture(e, e->mixed);
            size_t bytes = (size_t)e->mixed->params.w * e->mixed->params.h * 4;
            int maximum = 0;
            for (size_t p = 0; p < bytes; p++) {
                int expected = (int)lroundf(old[p] * (1 - progress) + next[p] * progress);
                int difference = abs((int)mixed[p] - expected);
                if (difference > maximum)
                    maximum = difference;
            }
            assert(maximum <= 1);
            free(mixed);
            printf("BLEND progress=%.1f max_error=%d\n", progress, maximum);
        }
        free(old);
        free(next);
        pump_for(e, .02);
    }
    clear_transition(e);
}
static void *cancel_later(void *opaque) {
    int fd = *(int *)opaque;
    struct timespec delay = {.tv_nsec = 150000000};
    nanosleep(&delay, NULL);
    assert(write(fd, "x", 1) == 1);
    return NULL;
}
static void drain(int fd) {
    char bytes[32];
    while (read(fd, bytes, sizeof(bytes)) > 0) {
    }
}
int main(int argc, char **argv) {
    if (argc != 6)
        return 2; /* red blue video output decode */
    setvbuf(stdout, NULL, _IOLBF, 0);
    int wake[2];
    assert(!pipe2(wake, O_NONBLOCK | O_CLOEXEC));
    struct lw_engine *e = lw_create(wake[0]);
    int decode = atoi(argv[5]);
    struct lw_options cut = {.fit = 0, .fps = 30};
    struct lw_options fade = {.fit = 1, .duration_ms = 1200, .fps = 30};
    assert(!lw_show(e, argv[1], argv[4], cut));
    uint64_t before = e->stats.frames;
    assert(!lw_show_video_transition(e, argv[3], argv[4], fade, decode));
    assert(e->stats.frames >= before + 20);
    printf("LIVE_FADE image_to_video frames=%lu position=%.3f\n",
           (unsigned long)(e->stats.frames - before),
           lw_clock_position(&e->video->clock, image_now()));
    assert(lw_clock_position(&e->video->clock, image_now()) >= 1.1);
    assert(e->video->loop >= 1); /* incoming actually advanced during fade */
    uint64_t frame_start = e->stats.frames;
    before = e->stats.video_loops;
    assert(!lw_show_video_transition(e, argv[3], argv[4], fade, decode));
    assert(e->stats.frames >= frame_start + 20);
    printf("LIVE_FADE video_to_video frames=%lu loops=%lu\n",
           (unsigned long)(e->stats.frames - frame_start),
           (unsigned long)(e->stats.video_loops - before));
    assert(e->stats.video_loops >= before + 2); /* both live video sources looped */
    int image = e->images[0].path ? 0 : 1;
    check_blend(e, image);
    for (int i = 0; i < 3; i++) {
        struct video_session *retained = e->video;
        fade.duration_ms = 2000;
        pthread_t cancel;
        assert(!pthread_create(&cancel, NULL, cancel_later, &wake[1]));
        double start = image_now();
        int result = i == 0 ? lw_show(e, argv[2], argv[4], fade)
                            : lw_show_video_transition(e, argv[3], argv[4], fade, decode);
        pthread_join(cancel, NULL);
        assert(result == -1 && e->video == retained && !e->fatal);
        assert(image_now() - start < 1.5);
        assert(!e->mixed && !e->video_scaled[0] && !e->video_scaled[1]);
        drain(wake[0]);
        before = e->stats.video_frames;
        pump_for(e, .15);
        assert(e->stats.video_frames > before);
    }
    fade.duration_ms = 400;
    before = e->stats.frames;
    assert(!lw_show(e, argv[2], argv[4], fade));
    assert(!e->video && e->stats.frames >= before + 4);
    assert(!e->mixed && !e->video_scaled[0] && !e->video_scaled[1]);
    for (int i = 0; i < 8; i++) {
        assert(!lw_show_video_transition(e, argv[3], argv[4], fade, decode));
        assert(!lw_show(e, i % 2 ? argv[1] : argv[2], argv[4], fade));
    }
    int retained_image = e->current;
    pthread_t cancel;
    fade.duration_ms = 2000;
    assert(!pthread_create(&cancel, NULL, cancel_later, &wake[1]));
    int cancelled = lw_show_video_transition(e, argv[3], argv[4], fade, decode);
    pthread_join(cancel, NULL);
    assert(cancelled == -1 && !e->video && e->current == retained_image && e->active);
    assert(!e->mixed && !e->video_scaled[0] && !e->video_scaled[1]);
    drain(wake[0]);
    lw_destroy(e);
    close(wake[0]);
    close(wake[1]);
    printf("MEDIA_TRANSITION_PASS decode=%d switches=19 cancelled=4\n", decode);
    return 0;
}

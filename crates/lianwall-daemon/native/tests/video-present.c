/* Opt-in timing prototype: reuse the real native image context, not a second WSI backend.
 * Software decoding uses a bounded worker queue; daemon integration is still pending. */
#include "../engine.c"
#include "video_clock.h"
#include "video_gpu.h"
#include "video_queue.h"
#include <fcntl.h>
#include <signal.h>
#include <unistd.h>

#define FEEDBACK_LIMIT 1024
struct measurement {
    struct wp_presentation_feedback *feedback;
    uint64_t index, loop;
    double pts, submitted, presented;
    uint32_t refresh, flags;
    bool discarded;
};
static struct measurement samples[FEEDBACK_LIMIT];
static unsigned submitted, delivered, discarded_count, dropped;
static struct lw_engine *context;
static volatile sig_atomic_t stopped;
static int signal_fd = -1;
static void stop_signal(int signal) {
    (void)signal;
    int saved = errno;
    stopped = 1;
    if (signal_fd >= 0) {
        char byte = 1;
        ssize_t unused = write(signal_fd, &byte, 1);
        (void)unused;
    }
    errno = saved;
}
static double feedback_to_monotonic(double timestamp) {
    struct timespec value;
    double before = image_now();
    if (clock_gettime((clockid_t)context->presentation_clock_id, &value))
        return NAN;
    double after = image_now();
    return timestamp + (before + after) / 2 - (value.tv_sec + value.tv_nsec / 1e9);
}
static void output_sync(void *data, struct wp_presentation_feedback *f, struct wl_output *output) {
    (void)data;
    (void)f;
    (void)output;
}
static void video_presented(void *data, struct wp_presentation_feedback *f, uint32_t hi,
                            uint32_t lo, uint32_t ns, uint32_t refresh, uint32_t shi, uint32_t slo,
                            uint32_t flags) {
    (void)shi;
    (void)slo;
    struct measurement *sample = data;
    double raw = (double)(((uint64_t)hi << 32) | lo) + ns / 1e9;
    sample->presented = feedback_to_monotonic(raw);
    sample->refresh = refresh;
    sample->flags = flags;
    wp_presentation_feedback_destroy(f);
    sample->feedback = NULL;
    delivered++;
}
static void video_discarded(void *data, struct wp_presentation_feedback *f) {
    struct measurement *sample = data;
    sample->discarded = true;
    sample->feedback = NULL;
    wp_presentation_feedback_destroy(f);
    discarded_count++;
}
static const struct wp_presentation_feedback_listener video_feedback = {
    output_sync, video_presented, video_discarded};
static int draw_video(struct lw_engine *e, struct lw_video_gpu *mapping, double pts, uint64_t loop,
                      uint64_t index) {
    int w = (e->width * e->scale + 119) / 120, h = (e->height * e->scale + 119) / 120;
    if (submitted >= FEEDBACK_LIMIT || w < 1 || h < 1 || w > 8192 || h > 8192 ||
        (int64_t)w * h > 16 * 1024 * 1024)
        return -1;
    wp_viewport_set_destination(e->viewport, e->width, e->height);
    if (!pl_swapchain_resize(e->swap, &w, &h))
        return -1;
    struct pl_swapchain_frame swap;
    if (!pl_swapchain_start_frame(e->swap, &swap))
        return -1;
    struct pl_frame target;
    pl_frame_from_swapchain(&target, &swap);
    if (lw_video_gpu_render(mapping, e->renderer, &target, true))
        return -1;
    clear_callbacks(e);
    e->callback = wl_surface_frame(e->surface);
    wl_callback_add_listener(e->callback, &frame_listener, e);
    struct measurement *sample = &samples[submitted];
    *sample =
        (struct measurement){.pts = pts, .loop = loop, .index = index, .submitted = image_now()};
    sample->feedback = wp_presentation_feedback(e->presentation, e->surface);
    wp_presentation_feedback_add_listener(sample->feedback, &video_feedback, sample);
    if (!pl_swapchain_submit_frame(e->swap))
        return -1;
    pl_swapchain_swap_buffers(e->swap);
    submitted++;
    e->dirty = 0;
    printf("SUBMIT frame=%u loop=%lu source=%lu pts=%.9f\n", submitted, (unsigned long)loop,
           (unsigned long)index, pts);
    fflush(stdout);
    return 0;
}
struct input {
    struct lw_video_queue *queue;
    double end;
};
static int read_next(struct input *input, struct lw_video_frame *frame, double *pts) {
    int result = lw_video_queue_pop(input->queue, frame, pts);
    if (result == LW_VIDEO_FRAME)
        input->end = *pts + frame->duration;
    return result;
}
/* Wait on decoder readiness only when no prefetched frame exists. Queued future
 * frames must not cause busy polling while the presentation clock catches up. */
static int video_events(struct lw_engine *e, struct input *input, int timeout, bool wait_input) {
    bool had_callback = e->callback != NULL;
    int result = events(e, 0);
    if (result || stopped)
        return result;
    /* A pending callback may already have made rendering ready. Do not sleep
     * again using the timeout computed while the callback was outstanding. */
    if (had_callback && !e->callback)
        return 0;
    struct pollfd fds[3] = {
        {.fd = e->display ? wl_display_get_fd(e->display) : -1, .events = POLLIN},
        {.fd = e->wake_fd, .events = POLLIN},
        {.fd = wait_input ? lw_video_queue_fd(input->queue) : -1, .events = POLLIN},
    };
    int n = poll(fds, 3, timeout);
    if (n < 0 && errno != EINTR)
        return -1;
    return events(e, 0);
}
int main(int argc, char **argv) {
    if (argc != 4)
        return 2;
    int loops = atoi(argv[3]);
    if (loops < 1 || loops > 10)
        return 2;
    int wake[2];
    if (pipe(wake))
        return 3;
    fcntl(wake[0], F_SETFL, O_NONBLOCK);
    fcntl(wake[1], F_SETFL, O_NONBLOCK);
    signal_fd = wake[1];
    struct sigaction action = {.sa_handler = stop_signal};
    sigemptyset(&action.sa_mask);
    sigaction(SIGINT, &action, NULL);
    sigaction(SIGTERM, &action, NULL);
    int result = 1;
    struct lw_engine *e = lw_create(wake[0]);
    context = e;
    struct input input = {.queue = lw_video_queue_create(argv[1], (struct lw_video_queue_options){
                                                                      .max_frames = 4,
                                                                      .max_bytes = 64 * 1024 * 1024,
                                                                      .loops = (uint64_t)loops})};
    struct lw_video_gpu *mapping = NULL;
    struct lw_video_frame frame = {0};
    double pts = 0;
    if (!e || !input.queue)
        goto cleanup;
    double prepare_deadline = image_now() + 8;
    int first;
    while ((first = read_next(&input, &frame, &pts)) == LW_VIDEO_AGAIN && !stopped &&
           image_now() < prepare_deadline)
        if (video_events(e, &input, 100, true) < 0)
            goto cleanup;
    if (first != LW_VIDEO_FRAME || stopped)
        goto cleanup;
    if (initialize(e, argv[2]))
        goto cleanup;
    if (e->presentation_clock_id == UINT32_MAX) {
        fprintf(stderr, "Presentation clock event not received\n");
        goto cleanup;
    }
    printf("CLOCK_ID %u\n", e->presentation_clock_id);
    mapping = lw_video_gpu_create(e->vk->gpu);
    if (!mapping || lw_video_gpu_map(mapping, frame.frame))
        goto cleanup;
    uint64_t loop = frame.loop, index = frame.index;
    lw_video_frame_clear(&frame);
    if (draw_video(e, mapping, pts, loop, index))
        goto cleanup;
    pl_gpu_finish(e->vk->gpu);
    double deadline = image_now() + 3;
    while (!samples[0].presented && !samples[0].discarded && !stopped && image_now() < deadline)
        if (events(e, 50) < 0)
            goto cleanup;
    if (stopped) {
        result = 0;
        goto cleanup;
    }
    if (!isfinite(samples[0].presented) || !samples[0].presented || samples[0].discarded)
        goto cleanup;
    struct lw_video_clock clock = {0};
    if (!lw_clock_start(&clock, samples[0].presented, 0))
        goto cleanup;
    double refresh = samples[0].refresh ? samples[0].refresh / 1e9 : 0;
    int ready = read_next(&input, &frame, &pts);
    if (ready < 0)
        goto cleanup;
    deadline = image_now() + 20;
    while (!stopped && image_now() < deadline) {
        double time = lw_clock_position(&clock, image_now());
        if (ready == LW_VIDEO_AGAIN) {
            ready = read_next(&input, &frame, &pts);
            if (ready < 0)
                goto cleanup;
        }
        if (ready == LW_VIDEO_EOF) {
            if (time >= input.end)
                break;
        } else if (ready == LW_VIDEO_FRAME && !e->callback && pts <= time + refresh) {
            // Select the newest due frame instead of emitting a catch-up burst.
            struct lw_video_frame selected = frame;
            frame = (struct lw_video_frame){0};
            double selected_pts = pts;
            ready = read_next(&input, &frame, &pts);
            if (ready < 0) {
                lw_video_frame_clear(&selected);
                goto cleanup;
            }
            while (ready == LW_VIDEO_FRAME &&
                   pts <= lw_clock_position(&clock, image_now()) + refresh) {
                lw_video_frame_clear(&selected);
                selected = frame;
                selected_pts = pts;
                frame = (struct lw_video_frame){0};
                dropped++;
                ready = read_next(&input, &frame, &pts);
                if (ready < 0) {
                    lw_video_frame_clear(&selected);
                    goto cleanup;
                }
            }
            loop = selected.loop;
            index = selected.index;
            if (lw_video_gpu_map(mapping, selected.frame)) {
                lw_video_frame_clear(&selected);
                goto cleanup;
            }
            lw_video_frame_clear(&selected);
            if (draw_video(e, mapping, selected_pts, loop, index))
                goto cleanup;
        } else if (e->dirty && !e->callback) {
            if (draw_video(e, mapping, submitted ? samples[submitted - 1].pts : 0, loop, index))
                goto cleanup;
        }
        int timeout =
            ready == LW_VIDEO_FRAME
                ? (int)fmax(
                      1, fmin(100, (pts - lw_clock_position(&clock, image_now()) - refresh) * 1000))
                : 10;
        if (e->callback)
            timeout = 100;
        if (video_events(e, &input, timeout, ready == LW_VIDEO_AGAIN) < 0)
            goto cleanup;
    }
    if (!stopped && image_now() >= deadline) {
        fprintf(stderr, "Playback exceeded test budget\n");
        goto cleanup;
    }
    deadline = image_now() + 1;
    while (!stopped && delivered + discarded_count < submitted && image_now() < deadline)
        if (events(e, 20) < 0)
            goto cleanup;
    result = e->gpu_errors ? 1 : 0;
    for (unsigned i = 0; i < submitted; i++) {
        struct measurement *m = &samples[i];
        printf("{\"event\":\"presentation\",\"frame\":%u,\"loop\":%lu,\"source\":%lu,\"pts\":%.9f,"
               "\"submitted\":%.9f,\"presented\":%.9f,\"refresh_ns\":%u,\"flags\":%u,\"discarded\":"
               "%s}\n",
               i, (unsigned long)m->loop, (unsigned long)m->index, m->pts, m->submitted,
               m->presented, m->refresh, m->flags, m->discarded ? "true" : "false");
    }
    printf("RESULT submitted=%u presented=%u discarded=%u dropped=%u errors=%d cancelled=%d "
           "duration=%.9f\n",
           submitted, delivered, discarded_count, dropped, e->gpu_errors, stopped ? 1 : 0,
           input.end);
cleanup:
    if (stopped) {
        result = 0;
        printf("CANCELLED submitted=%u\n", submitted);
    }
    char queue_error[512];
    lw_video_queue_error(input.queue, queue_error, sizeof(queue_error));
    if (result && e && !*lw_error(e) && !*queue_error &&
        (!mapping || !*lw_video_gpu_error(mapping)))
        fail(e, e->closed ? "Selected output or wallpaper layer closed"
                          : "Video presentation or Wayland event processing failed");
    if (result && e)
        fprintf(stderr, "PLAYBACK_ERROR %s %s %s\n", lw_error(e), queue_error,
                mapping ? lw_video_gpu_error(mapping) : "");
    for (unsigned i = 0; i < FEEDBACK_LIMIT; i++)
        if (samples[i].feedback)
            wp_presentation_feedback_destroy(samples[i].feedback);
    lw_video_frame_clear(&frame);
    if (input.queue) {
        struct lw_video_queue_stats stats;
        lw_video_queue_stats(input.queue, &stats);
        printf("QUEUE peak_frames=%u peak_bytes=%zu waits=%lu produced=%lu\n", stats.peak_frames,
               stats.peak_bytes, (unsigned long)stats.waits, (unsigned long)stats.produced);
    }
    lw_video_queue_destroy(input.queue);
    if (e && e->vk)
        pl_gpu_finish(e->vk->gpu);
    lw_video_gpu_destroy(mapping);
    lw_destroy(e);
    close(wake[0]);
    close(wake[1]);
    signal_fd = -1;
    printf("CLEANUP result=%d\n", result);
    return result;
}

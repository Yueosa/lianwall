#define _GNU_SOURCE
#include "video_queue.h"
#include <errno.h>
#include <fcntl.h>
#include <libavutil/buffer.h>
#include <math.h>
#include <pthread.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

struct slot {
    struct lw_video_frame frame;
    double pts;
    size_t bytes;
};
struct lw_video_queue {
    pthread_t thread;
    pthread_mutex_t lock;
    pthread_cond_t space;
    atomic_bool cancelled;
    int pipe[2], terminal;
    char *path, error[512];
    struct lw_video_queue_options options;
    struct lw_video_queue_hardware hardware;
    char fallback_reason[512];
    struct lw_video_queue_stats stats;
    struct slot slots[16];
    unsigned head;
};
static bool cancelled(void *opaque) {
    struct lw_video_queue *q = opaque;
    return atomic_load(&q->cancelled);
}
/* Coalesced level notification; all queue state transitions hold lock. */
static void notify(struct lw_video_queue *q) {
    char byte = 1;
    ssize_t result;
    do {
        result = write(q->pipe[1], &byte, 1);
    } while (result < 0 && errno == EINTR);
}
static size_t frame_bytes(const AVFrame *f) {
    size_t total = sizeof(*f);
#define CHARGE(n)                                                                                  \
    do {                                                                                           \
        size_t size = (n);                                                                         \
        if (size > SIZE_MAX - total)                                                               \
            return SIZE_MAX;                                                                       \
        total += size;                                                                             \
    } while (0)
    for (unsigned i = 0; i < AV_NUM_DATA_POINTERS; i++)
        if (f->buf[i])
            CHARGE(f->buf[i]->size);
    for (int i = 0; i < f->nb_extended_buf; i++)
        CHARGE(f->extended_buf[i]->size);
    for (int i = 0; i < f->nb_side_data; i++) {
        CHARGE(sizeof(*f->side_data[i]));
        CHARGE(f->side_data[i]->size);
    }
#undef CHARGE
    return total;
}
static void *produce(void *opaque) {
    struct lw_video_queue *q = opaque;
    struct lw_video *video = NULL;
    struct lw_video_frame frame = {0};
    double base = 0, last_end = 0;
    uint64_t completed = 0;
    int result = LW_VIDEO_ERROR;
    char error[512] = "Video decoder allocation failed";
    bool hardware = q->hardware.device != NULL, published = false;
start:
    video = lw_video_create(cancelled, q);
    if (!video)
        goto finish;
    if (hardware &&
        lw_video_set_hardware(video, q->hardware.device, q->hardware.check, q->hardware.opaque))
        goto finish;
    if (hardware && q->hardware.prepare && lw_video_set_hardware_frames(video, q->hardware.prepare))
        goto finish;
    result = lw_video_open(video, q->path);
    if (result)
        goto finish;
    for (;;) {
        result = lw_video_next(video, &frame);
        if (result == LW_VIDEO_EOF) {
            if (++completed == q->options.loops)
                break;
            if (!(last_end > 0) || !isfinite(base + last_end) || base + last_end <= base) {
                snprintf(error, sizeof(error), "Invalid loop timeline");
                result = LW_VIDEO_ERROR;
                goto finish;
            }
            base += last_end;
            last_end = 0;
            result = lw_video_rewind(video);
            if (result)
                goto finish;
            continue;
        }
        if (result != LW_VIDEO_FRAME)
            break;
        size_t bytes = frame_bytes(frame.frame);
        if (hardware) {
            size_t gpu_bytes = q->hardware.frame_bytes(frame.frame);
            bytes = gpu_bytes > SIZE_MAX - bytes ? SIZE_MAX : bytes + gpu_bytes;
        }
        if (bytes > q->options.max_bytes) {
            snprintf(error, sizeof(error), "Decoded frame exceeds queue byte budget");
            result = LW_VIDEO_ERROR;
            goto finish;
        }
        last_end = frame.pts + frame.duration;
        double pts = base + frame.pts;
        if (!isfinite(pts) || !isfinite(last_end)) {
            snprintf(error, sizeof(error), "Invalid video queue timestamp");
            result = LW_VIDEO_ERROR;
            goto finish;
        }
        pthread_mutex_lock(&q->lock);
        while (!cancelled(q) && (q->stats.frames == q->options.max_frames ||
                                 bytes > q->options.max_bytes - q->stats.bytes)) {
            q->stats.waits++;
            pthread_cond_wait(&q->space, &q->lock);
        }
        if (cancelled(q)) {
            pthread_mutex_unlock(&q->lock);
            result = LW_VIDEO_CANCELLED;
            break;
        }
        unsigned tail = (q->head + q->stats.frames) % q->options.max_frames;
        q->slots[tail] = (struct slot){.frame = frame, .pts = pts, .bytes = bytes};
        frame = (struct lw_video_frame){0};
        q->stats.frames++;
        q->stats.bytes += bytes;
        q->stats.produced++;
        if (hardware)
            q->stats.hardware_frames++;
        else
            q->stats.software_frames++;
        published = true;
        if (q->stats.frames > q->stats.peak_frames)
            q->stats.peak_frames = q->stats.frames;
        if (q->stats.bytes > q->stats.peak_bytes)
            q->stats.peak_bytes = q->stats.bytes;
        notify(q);
        pthread_mutex_unlock(&q->lock);
    }
finish:
    if (hardware && q->hardware.fallback && !published && result == LW_VIDEO_ERROR &&
        !cancelled(q)) {
        pthread_mutex_lock(&q->lock);
        q->stats.fallbacks++;
        snprintf(q->fallback_reason, sizeof(q->fallback_reason), "%s",
                 video && *lw_video_error(video) ? lw_video_error(video) : error);
        pthread_mutex_unlock(&q->lock);
        lw_video_frame_clear(&frame);
        lw_video_destroy(video);
        video = NULL;
        hardware = false;
        base = 0;
        last_end = 0;
        completed = 0;
        result = LW_VIDEO_ERROR;
        snprintf(error, sizeof(error), "Software fallback allocation failed");
        goto start;
    }
    lw_video_frame_clear(&frame);
    if (video && *lw_video_error(video))
        snprintf(error, sizeof(error), "%s", lw_video_error(video));
    lw_video_destroy(video);
    pthread_mutex_lock(&q->lock);
    q->terminal = cancelled(q) ? LW_VIDEO_CANCELLED : result;
    if (q->terminal < 0)
        snprintf(q->error, sizeof(q->error), "%s", error);
    notify(q);
    pthread_mutex_unlock(&q->lock);
    return NULL;
}
struct lw_video_queue *lw_video_queue_create(const char *path,
                                             struct lw_video_queue_options options) {
    return lw_video_queue_create_hardware(path, options, (struct lw_video_queue_hardware){0});
}
struct lw_video_queue *lw_video_queue_create_hardware(const char *path,
                                                      struct lw_video_queue_options options,
                                                      struct lw_video_queue_hardware hardware) {
    if (!path || !options.max_frames || options.max_frames > 16 || !options.max_bytes ||
        options.max_bytes > 256 * 1024 * 1024)
        return NULL;
    if (hardware.device && (!hardware.check || !hardware.frame_bytes))
        return NULL;
    struct lw_video_queue *q = calloc(1, sizeof(*q));
    if (!q)
        return NULL;
    q->options = options;
    q->hardware = hardware;
    if (hardware.device) {
        q->hardware.device = av_buffer_ref(hardware.device);
        if (!q->hardware.device)
            goto free_queue;
    }
    q->terminal = LW_VIDEO_AGAIN;
    atomic_init(&q->cancelled, false);
    q->path = strdup(path);
    if (!q->path)
        goto free_queue;
    if (pipe2(q->pipe, O_NONBLOCK | O_CLOEXEC))
        goto free_path;
    if (pthread_mutex_init(&q->lock, NULL))
        goto close_pipe;
    if (pthread_cond_init(&q->space, NULL))
        goto destroy_lock;
    if (pthread_create(&q->thread, NULL, produce, q))
        goto destroy_cond;
    return q;
destroy_cond:
    pthread_cond_destroy(&q->space);
destroy_lock:
    pthread_mutex_destroy(&q->lock);
close_pipe:
    close(q->pipe[0]);
    close(q->pipe[1]);
free_path:
    free(q->path);
free_queue:
    av_buffer_unref(&q->hardware.device);
    free(q);
    return NULL;
}
int lw_video_queue_pop(struct lw_video_queue *q, struct lw_video_frame *output,
                       double *timeline_pts) {
    if (!q || !output || output->frame || !timeline_pts)
        return LW_VIDEO_ERROR;
    pthread_mutex_lock(&q->lock);
    char bytes[128];
    ssize_t n;
    do {
        n = read(q->pipe[0], bytes, sizeof(bytes));
    } while (n > 0 || (n < 0 && errno == EINTR));
    int result = q->terminal;
    if (cancelled(q))
        result = LW_VIDEO_CANCELLED;
    else if (q->stats.frames) {
        struct slot *slot = &q->slots[q->head];
        *output = slot->frame;
        *timeline_pts = slot->pts;
        q->stats.bytes -= slot->bytes;
        *slot = (struct slot){0};
        q->head = (q->head + 1) % q->options.max_frames;
        q->stats.frames--;
        pthread_cond_signal(&q->space);
        result = LW_VIDEO_FRAME;
    }
    if (q->stats.frames || q->terminal != LW_VIDEO_AGAIN || cancelled(q))
        notify(q);
    pthread_mutex_unlock(&q->lock);
    return result;
}
int lw_video_queue_fd(const struct lw_video_queue *q) { return q ? q->pipe[0] : -1; }
void lw_video_queue_stats(struct lw_video_queue *q, struct lw_video_queue_stats *stats) {
    pthread_mutex_lock(&q->lock);
    *stats = q->stats;
    pthread_mutex_unlock(&q->lock);
}
void lw_video_queue_error(struct lw_video_queue *q, char *output, size_t size) {
    if (!q) {
        snprintf(output, size, "Video queue allocation failed");
        return;
    }
    pthread_mutex_lock(&q->lock);
    snprintf(output, size, "%s", q->error);
    pthread_mutex_unlock(&q->lock);
}
void lw_video_queue_diagnostic(struct lw_video_queue *q, char *output, size_t size) {
    pthread_mutex_lock(&q->lock);
    snprintf(output, size, "%s", q->fallback_reason);
    pthread_mutex_unlock(&q->lock);
}
void lw_video_queue_cancel(struct lw_video_queue *q) {
    if (!q)
        return;
    atomic_store(&q->cancelled, true);
    pthread_mutex_lock(&q->lock);
    pthread_cond_broadcast(&q->space);
    notify(q);
    pthread_mutex_unlock(&q->lock);
}
void lw_video_queue_destroy(struct lw_video_queue *q) {
    if (!q)
        return;
    lw_video_queue_cancel(q);
    pthread_join(q->thread, NULL);
    for (unsigned i = 0; i < q->options.max_frames; i++)
        lw_video_frame_clear(&q->slots[i].frame);
    av_buffer_unref(&q->hardware.device);
    close(q->pipe[0]);
    close(q->pipe[1]);
    pthread_cond_destroy(&q->space);
    pthread_mutex_destroy(&q->lock);
    free(q->path);
    free(q);
}

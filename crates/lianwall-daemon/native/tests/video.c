#define _GNU_SOURCE
#include "video.h"
#include "video_clock.h"
#include <dirent.h>
#include <libavutil/imgutils.h>
#include <libavutil/md5.h>
#include <libavutil/pixdesc.h>
#include <pthread.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

static double now(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return ts.tv_sec + ts.tv_nsec / 1e9;
}
static int fd_count(void) {
    DIR *d = opendir("/proc/self/fd");
    if (!d)
        return -1;
    int count = 0;
    struct dirent *entry;
    while ((entry = readdir(d)))
        if (entry->d_name[0] != '.')
            count++;
    closedir(d);
    return count;
}
static long rss_kib(void) {
    FILE *f = fopen("/proc/self/statm", "r");
    long total = 0, resident = 0;
    if (!f)
        return -1;
    if (fscanf(f, "%ld %ld", &total, &resident) != 2)
        resident = -1;
    fclose(f);
    return resident < 0 ? -1 : resident * sysconf(_SC_PAGESIZE) / 1024;
}
static bool cancel(void *opaque) { return atomic_load((atomic_bool *)opaque); }
static int digest(const AVFrame *frame, char hex[33]) {
    const AVPixFmtDescriptor *desc = av_pix_fmt_desc_get(frame->format);
    if (!desc || (desc->flags & AV_PIX_FMT_FLAG_HWACCEL))
        return -1;
    int size = av_image_get_buffer_size(frame->format, frame->width, frame->height, 1);
    if (size < 1 || size > 256 * 1024 * 1024)
        return -1;
    uint8_t *packed = av_malloc(size);
    if (!packed)
        return -1;
    int copied =
        av_image_copy_to_buffer(packed, size, (const uint8_t *const *)frame->data, frame->linesize,
                                frame->format, frame->width, frame->height, 1);
    uint8_t hash[16];
    if (copied == size)
        av_md5_sum(hash, packed, size);
    av_free(packed);
    if (copied != size)
        return -1;
    for (int i = 0; i < 16; i++)
        snprintf(hex + i * 2, 3, "%02x", hash[i]);
    return 0;
}
static int decode(const char *path, int loops, bool print) {
    atomic_bool stop = false;
    struct lw_video *v = lw_video_create(cancel, &stop);
    if (!v)
        return 10;
    int result = lw_video_open(v, path);
    if (result < 0) {
        fprintf(stderr, "%s\n", lw_video_error(v));
        lw_video_destroy(v);
        return 11;
    }
    for (int loop = 0; loop < loops; loop++) {
        unsigned frames = 0;
        struct lw_video_frame out = {0};
        while ((result = lw_video_next(v, &out)) == LW_VIDEO_FRAME) {
            char md5[33];
            if (out.loop != (uint64_t)loop || out.index != frames || digest(out.frame, md5)) {
                lw_video_frame_clear(&out);
                lw_video_destroy(v);
                return 12;
            }
            if (print)
                printf("{\"event\":\"frame\",\"loop\":%d,\"index\":%u,\"pts\":%.12f,\"duration\":%."
                       "12f,\"synthetic\":%s,\"estimated_duration\":%s,\"width\":%d,\"height\":%d,"
                       "\"format\":\"%s\",\"md5\":\"%s\"}\n",
                       loop, frames, out.pts, out.duration, out.pts_synthesized ? "true" : "false",
                       out.duration_estimated ? "true" : "false", out.frame->width,
                       out.frame->height, av_get_pix_fmt_name(out.frame->format), md5);
            frames++;
            lw_video_frame_clear(&out);
        }
        if (result != LW_VIDEO_EOF || !frames) {
            fprintf(stderr, "%s\n", lw_video_error(v));
            lw_video_destroy(v);
            return 13;
        }
        if (lw_video_next(v, &out) != LW_VIDEO_EOF) {
            lw_video_destroy(v);
            return 14;
        }
        if (print)
            printf("{\"event\":\"eof\",\"loop\":%d,\"frames\":%u}\n", loop, frames);
        if (loop + 1 < loops && lw_video_rewind(v)) {
            fprintf(stderr, "rewind: %s\n", lw_video_error(v));
            lw_video_destroy(v);
            return 15;
        }
    }
    lw_video_destroy(v);
    return 0;
}
static int clocks(void) {
    struct lw_video_clock c = {0};
    if (lw_clock_position(&c, 90) != 0 || lw_clock_start(&c, NAN, 0))
        return 20;
    if (!lw_clock_start(&c, 100, 0) || fabs(lw_clock_position(&c, 100.25) - .25) > 1e-10)
        return 21;
    lw_clock_pause(&c, 100.5);
    lw_clock_pause(&c, 101);
    if (lw_clock_position(&c, 105) != .5)
        return 22;
    lw_clock_resume(&c, 110);
    if (fabs(lw_clock_position(&c, 110.2) - .7) > 1e-10)
        return 23;
    lw_clock_resume(&c, 111);
    if (fabs(lw_clock_position(&c, 111) - 1.5) > 1e-10)
        return 24;
    if (!lw_clock_start(&c, 120, 0) || lw_clock_position(&c, 120) != 0)
        return 25;
    printf("{\"event\":\"clock\",\"result\":\"pass\"}\n");
    return 0;
}
/* Reference transport harness: four owned AVFrames, explicit backpressure and cancellation.
 * This does not claim the future GPU/decoder production queue is already integrated. */
struct queue {
    pthread_mutex_t mutex;
    pthread_cond_t changed;
    atomic_bool cancel;
    struct lw_video_frame frames[4];
    const char *path;
    unsigned head, count, peak, wait_count;
    int result;
    bool done;
};
static void *producer(void *opaque) {
    struct queue *q = opaque;
    struct lw_video *v = lw_video_create(cancel, &q->cancel);
    int result = v ? lw_video_open(v, q->path) : LW_VIDEO_ERROR;
    if (!result)
        for (;;) {
            struct lw_video_frame frame = {0};
            result = lw_video_next(v, &frame);
            if (result != LW_VIDEO_FRAME)
                break;
            pthread_mutex_lock(&q->mutex);
            while (q->count == 4 && !atomic_load(&q->cancel)) {
                q->wait_count++;
                pthread_cond_broadcast(&q->changed);
                pthread_cond_wait(&q->changed, &q->mutex);
            }
            if (atomic_load(&q->cancel)) {
                pthread_mutex_unlock(&q->mutex);
                lw_video_frame_clear(&frame);
                result = LW_VIDEO_CANCELLED;
                break;
            }
            q->frames[(q->head + q->count) % 4] = frame;
            q->count++;
            if (q->count > q->peak)
                q->peak = q->count;
            pthread_cond_broadcast(&q->changed);
            pthread_mutex_unlock(&q->mutex);
        }
    lw_video_destroy(v);
    pthread_mutex_lock(&q->mutex);
    q->result = result;
    q->done = true;
    pthread_cond_broadcast(&q->changed);
    pthread_mutex_unlock(&q->mutex);
    return NULL;
}
static int queue_cancel(const char *path) {
    struct queue q = {.path = path};
    atomic_init(&q.cancel, false);
    pthread_mutex_init(&q.mutex, NULL);
    pthread_condattr_t attributes;
    pthread_condattr_init(&attributes);
    pthread_condattr_setclock(&attributes, CLOCK_MONOTONIC);
    pthread_cond_init(&q.changed, &attributes);
    pthread_condattr_destroy(&attributes);
    pthread_t worker;
    if (pthread_create(&worker, NULL, producer, &q))
        return 30;
    struct timespec deadline;
    clock_gettime(CLOCK_MONOTONIC, &deadline);
    deadline.tv_sec += 3;
    pthread_mutex_lock(&q.mutex);
    int failed = 0;
    while (!q.wait_count && !q.done) {
        if (pthread_cond_timedwait(&q.changed, &q.mutex, &deadline)) {
            failed = 1;
            break;
        }
    }
    if (!failed && !q.done) {
        unsigned waits = q.wait_count;
        lw_video_frame_clear(&q.frames[q.head]);
        q.head = (q.head + 1) % 4;
        q.count--;
        pthread_cond_broadcast(&q.changed);
        while (q.wait_count == waits && !q.done)
            if (pthread_cond_timedwait(&q.changed, &q.mutex, &deadline)) {
                failed = 1;
                break;
            }
    } else
        failed = 1;
    double start = now();
    atomic_store(&q.cancel, true);
    pthread_cond_broadcast(&q.changed);
    pthread_mutex_unlock(&q.mutex);
    pthread_join(worker, NULL);
    double latency = now() - start;
    if (q.result != LW_VIDEO_CANCELLED || q.peak != 4 || latency > 2)
        failed = 1;
    // Queue references must remain readable after their decoder was destroyed.
    for (int i = 0; i < 4; i++) {
        char hash[33];
        if (q.frames[i].frame && digest(q.frames[i].frame, hash))
            failed = 1;
        lw_video_frame_clear(&q.frames[i]);
    }
    pthread_mutex_destroy(&q.mutex);
    pthread_cond_destroy(&q.changed);
    printf("{\"event\":\"backpressure_cancel\",\"peak\":%u,\"wait_count\":%u,\"latency_ms\":%.4f,"
           "\"result\":\"%s\"}\n",
           q.peak, q.wait_count, latency * 1000, failed ? "fail" : "pass");
    return failed ? 31 : 0;
}
struct stalled_interrupt {
    bool stall;
};
static bool stall_once(void *opaque) {
    struct stalled_interrupt *state = opaque;
    if (state->stall) {
        state->stall = false;
        struct timespec pause = {.tv_sec = 3, .tv_nsec = 30000000};
        nanosleep(&pause, NULL);
    }
    return false;
}
static int timeout_check(const char *path) {
    struct stalled_interrupt state = {0};
    struct lw_video *v = lw_video_create(stall_once, &state);
    if (!v || lw_video_open(v, path)) {
        lw_video_destroy(v);
        return 44;
    }
    state.stall = true;
    struct lw_video_frame frame = {0};
    int result = lw_video_next(v, &frame);
    lw_video_frame_clear(&frame);
    lw_video_destroy(v);
    if (result != LW_VIDEO_TIMEOUT)
        return 45;
    printf("{\"event\":\"cooperative_timeout\",\"result\":\"pass\"}\n");
    return 0;
}
static int cancellation(const char *path) {
    atomic_bool stop = true;
    struct lw_video *v = lw_video_create(cancel, &stop);
    if (!v || lw_video_open(v, path) != LW_VIDEO_CANCELLED) {
        lw_video_destroy(v);
        return 40;
    }
    lw_video_destroy(v);
    atomic_store(&stop, false);
    v = lw_video_create(cancel, &stop);
    if (!v || lw_video_open(v, path)) {
        lw_video_destroy(v);
        return 41;
    }
    struct lw_video_frame frame = {0};
    if (lw_video_next(v, &frame) != LW_VIDEO_FRAME) {
        lw_video_destroy(v);
        return 42;
    }
    lw_video_frame_clear(&frame);
    atomic_store(&stop, true);
    int result = lw_video_next(v, &frame);
    lw_video_frame_clear(&frame);
    lw_video_destroy(v);
    if (result != LW_VIDEO_CANCELLED)
        return 43;
    printf("{\"event\":\"decode_cancel\",\"result\":\"pass\"}\n");
    return 0;
}
int main(int argc, char **argv) {
    av_log_set_level(AV_LOG_ERROR);
    if (argc == 4 && !strcmp(argv[1], "decode"))
        return decode(argv[2], atoi(argv[3]), true);
    if (argc == 3 && !strcmp(argv[1], "reject")) {
        struct lw_video *v = lw_video_create(NULL, NULL);
        if (!v)
            return 50;
        int rc = lw_video_open(v, argv[2]);
        if (!rc) {
            struct lw_video_frame f = {0};
            while ((rc = lw_video_next(v, &f)) == LW_VIDEO_FRAME)
                lw_video_frame_clear(&f);
            lw_video_frame_clear(&f);
        }
        printf("{\"event\":\"reject\",\"code\":%d,\"error\":\"%s\"}\n", rc, lw_video_error(v));
        lw_video_destroy(v);
        return rc < 0 ? 0 : 51;
    }
    if (argc == 3 && !strcmp(argv[1], "check")) {
        int rc = clocks();
        if (rc)
            return rc;
        rc = cancellation(argv[2]);
        if (rc)
            return rc;
        rc = timeout_check(argv[2]);
        if (rc)
            return rc;
        rc = queue_cancel(argv[2]);
        if (rc)
            return rc;
        int baseline = fd_count();
        long warm_rss = 0;
        for (int i = 0; i < 50; i++) {
            rc = decode(argv[2], 2, false);
            if (rc)
                return rc;
            if (i == 9)
                warm_rss = rss_kib();
        }
        long end_rss = rss_kib();
        int end_fd = fd_count();
        printf("{\"event\":\"repeat\",\"opens\":50,\"loops\":100,\"fd_before\":%d,\"fd_after\":%d,"
               "\"rss_warm_kib\":%ld,\"rss_end_kib\":%ld}\n",
               baseline, end_fd, warm_rss, end_rss);
        if (baseline != end_fd)
            return 60;
#ifndef __SANITIZE_ADDRESS__
        if (end_rss - warm_rss > 32768)
            return 61;
#endif
        return 0;
    }
    return 2;
}

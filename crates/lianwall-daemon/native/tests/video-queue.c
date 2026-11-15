#define _GNU_SOURCE
#include "video_queue.h"
#include <dirent.h>
#include <libavutil/imgutils.h>
#include <libavutil/md5.h>
#include <math.h>
#include <poll.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>
#define CHECK(c)                                                                                   \
    do {                                                                                           \
        if (!(c)) {                                                                                \
            fprintf(stderr, "check failed line %d: %s\n", __LINE__, #c);                           \
            exit(1);                                                                               \
        }                                                                                          \
    } while (0)
static double now(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return ts.tv_sec + ts.tv_nsec / 1e9;
}
static void pause_ms(unsigned ms) {
    struct timespec t = {.tv_sec = ms / 1000, .tv_nsec = (ms % 1000) * 1000000};
    nanosleep(&t, NULL);
}
static int fd_count(void) {
    DIR *dir = opendir("/proc/self/fd");
    CHECK(dir);
    int count = 0;
    struct dirent *entry;
    while ((entry = readdir(dir)))
        if (entry->d_name[0] != '.')
            count++;
    closedir(dir);
    return count;
}
static void digest(const AVFrame *f, unsigned char hash[16]) {
    int size = av_image_get_buffer_size(f->format, f->width, f->height, 1);
    CHECK(size > 0);
    uint8_t *data = malloc(size);
    CHECK(data);
    CHECK(av_image_copy_to_buffer(data, size, (const uint8_t *const *)f->data, f->linesize,
                                  f->format, f->width, f->height, 1) == size);
    av_md5_sum(hash, data, size);
    free(data);
}
static int next(struct lw_video_queue *q, struct lw_video_frame *frame, double *pts) {
    double deadline = now() + 8;
    for (;;) {
        int r = lw_video_queue_pop(q, frame, pts);
        if (r != LW_VIDEO_AGAIN)
            return r;
        CHECK(now() < deadline);
        struct pollfd fd = {.fd = lw_video_queue_fd(q), .events = POLLIN};
        CHECK(poll(&fd, 1, 100) >= 0);
    }
}
static struct lw_video_queue *create(const char *path, unsigned frames, size_t bytes,
                                     uint64_t loops) {
    struct lw_video_queue *q =
        lw_video_queue_create(path, (struct lw_video_queue_options){frames, bytes, loops});
    CHECK(q);
    return q;
}
static struct lw_video_queue_stats wait_full(struct lw_video_queue *q, unsigned frames) {
    double deadline = now() + 8;
    struct lw_video_queue_stats s;
    do {
        lw_video_queue_stats(q, &s);
        if (s.frames == frames && s.waits)
            return s;
        CHECK(now() < deadline);
        pause_ms(1);
    } while (1);
}
static void compare(const char *path, unsigned loops) {
    struct lw_video_queue *q = create(path, 4, 64 * 1024 * 1024, loops);
    struct lw_video *sync = lw_video_create(NULL, NULL);
    CHECK(sync && !lw_video_open(sync, path));
    struct lw_video_frame a = {0}, b = {0};
    double pts = 0, base = 0, end = 0, previous = -1;
    unsigned total = 0;
    for (unsigned loop = 0; loop < loops; loop++) {
        int r;
        while ((r = lw_video_next(sync, &a)) == LW_VIDEO_FRAME) {
            CHECK(next(q, &b, &pts) == LW_VIDEO_FRAME);
            CHECK(a.index == b.index && b.loop == loop && pts > previous);
            CHECK(fabs(pts - (base + a.pts)) < 1e-9);
            unsigned char ah[16], bh[16];
            digest(a.frame, ah);
            digest(b.frame, bh);
            CHECK(!memcmp(ah, bh, 16));
            CHECK(a.frame->width == b.frame->width && a.frame->height == b.frame->height &&
                  a.frame->format == b.frame->format);
            previous = pts;
            end = a.pts + a.duration;
            total++;
            lw_video_frame_clear(&a);
            lw_video_frame_clear(&b);
        }
        CHECK(r == LW_VIDEO_EOF);
        base += end;
        if (loop + 1 < loops)
            CHECK(!lw_video_rewind(sync));
    }
    CHECK(next(q, &b, &pts) == LW_VIDEO_EOF);
    struct lw_video_queue_stats s;
    lw_video_queue_stats(q, &s);
    CHECK(s.frames == 0 && s.bytes == 0 && s.peak_frames <= 4 && s.peak_bytes <= 64 * 1024 * 1024 &&
          s.produced == total);
    lw_video_destroy(sync);
    lw_video_queue_destroy(q);
    printf("{\"event\":\"compare\",\"frames\":%u,\"result\":\"pass\"}\n", total);
}
static void lifecycle(const char *path) {
    struct lw_video_queue *q = create(path, 1, 64 * 1024 * 1024, 0);
    struct lw_video_queue_stats s = wait_full(q, 1), later;
    size_t one = s.bytes;
    pause_ms(30);
    lw_video_queue_stats(q, &later);
    CHECK(later.produced == s.produced && later.frames == 1);
    struct lw_video_frame held = {0};
    double pts = 0;
    CHECK(next(q, &held, &pts) == LW_VIDEO_FRAME);
    unsigned char before[16], after[16];
    digest(held.frame, before);
    wait_full(q, 1);
    double start = now();
    lw_video_queue_cancel(q);
    struct pollfd fd = {.fd = lw_video_queue_fd(q), .events = POLLIN};
    CHECK(poll(&fd, 1, 100) == 1);
    struct lw_video_frame empty = {0};
    CHECK(lw_video_queue_pop(q, &empty, &pts) == LW_VIDEO_CANCELLED);
    lw_video_queue_destroy(q);
    double latency = (now() - start) * 1000;
    CHECK(latency < 1000);
    digest(held.frame, after);
    CHECK(!memcmp(before, after, 16));
    lw_video_frame_clear(&held);
    q = create(path, 4, one, 0);
    s = wait_full(q, 1);
    CHECK(s.peak_frames == 1 && s.peak_bytes <= one);
    lw_video_queue_destroy(q);
    q = create(path, 4, 1, 1);
    CHECK(next(q, &empty, &pts) == LW_VIDEO_ERROR);
    char error[512];
    lw_video_queue_error(q, error, sizeof(error));
    CHECK(strstr(error, "byte budget"));
    lw_video_queue_destroy(q);
    q = create("/nonexistent/lianwall-video.mp4", 4, 64 * 1024 * 1024, 1);
    CHECK(next(q, &empty, &pts) == LW_VIDEO_ERROR);
    lw_video_queue_error(q, error, sizeof(error));
    CHECK(*error);
    lw_video_queue_destroy(q);
    CHECK(!lw_video_queue_create(path, (struct lw_video_queue_options){0, 1, 1}));
    CHECK(!lw_video_queue_create(path, (struct lw_video_queue_options){17, 1, 1}));
    CHECK(!lw_video_queue_create(path, (struct lw_video_queue_options){1, 257UL * 1024 * 1024, 1}));
    int fds = fd_count();
    for (unsigned i = 0; i < 100; i++) {
        q = create(path, 4, 64 * 1024 * 1024, 0);
        if (i % 2 == 0)
            wait_full(q, 4);
        lw_video_queue_destroy(q);
    }
    CHECK(fd_count() == fds);
    printf("{\"event\":\"lifecycle\",\"cancel_join_ms\":%.4f,\"fd_before\":%d,\"fd_after\":%d,"
           "\"single_frame_bytes\":%zu,\"result\":\"pass\"}\n",
           latency, fds, fd_count(), one);
}
int main(int argc, char **argv) {
    if (argc != 3 && argc != 4)
        return 2;
    if (!strcmp(argv[1], "compare"))
        compare(argv[2], argc == 4 ? (unsigned)atoi(argv[3]) : 3);
    else if (!strcmp(argv[1], "lifecycle"))
        lifecycle(argv[2]);
    else
        return 2;
    return 0;
}

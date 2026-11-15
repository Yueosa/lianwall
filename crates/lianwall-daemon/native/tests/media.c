/* Direct native owner lifecycle probe, including the daemon's production video path. */
#define _GNU_SOURCE
#include "engine.h"
#include <assert.h>
#include <fcntl.h>
#include <stdio.h>
#include <time.h>
#include <unistd.h>
static double now(void) {
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return t.tv_sec + t.tv_nsec / 1e9;
}
static void pump(struct lw_engine *e, double seconds) {
    double end = now() + seconds;
    while (now() < end)
        assert(lw_pump(e, 10) >= 0);
}
int main(int argc, char **argv) {
    if (argc != 4)
        return 2;
    int wake[2];
    assert(!pipe2(wake, O_NONBLOCK | O_CLOEXEC));
    struct lw_engine *e = lw_create(wake[0]);
    assert(e);
    struct lw_options cut = {.fit = 0, .fps = 60};
    assert(!lw_show(e, argv[1], argv[3], cut));
    assert(!lw_show_video(e, argv[2], argv[3], 0));
    pump(e, 2.6);
    struct lw_stats playing = lw_statistics(e);
    assert(playing.video_frames >= 30 && playing.video_loops >= 2);
    assert(lw_show_video(e, "/nonexistent/wall.mp4", argv[3], 0) == -1);
    pump(e, .1);
    assert(lw_statistics(e).video_frames > playing.video_frames);
    for (int i = 0; i < 20; i++) {
        assert(!lw_show(e, argv[1], argv[3], cut));
        uint64_t count = lw_statistics(e).video_frames;
        pump(e, .02);
        assert(lw_statistics(e).video_frames == count);
        assert(!lw_show_video(e, argv[2], argv[3], i % 2));
        pump(e, .02);
    }
    lw_hide(e);
    assert(!lw_show_video(e, argv[2], argv[3], 0));
    lw_destroy(e);
    close(wake[0]);
    close(wake[1]);
    printf("MEDIA_LIFECYCLE_PASS switches=40 loops=%lu video_frames=%lu dropped=%lu\n",
           (unsigned long)playing.video_loops, (unsigned long)playing.video_frames,
           (unsigned long)playing.video_dropped);
    fflush(stdout);
    return 0;
}

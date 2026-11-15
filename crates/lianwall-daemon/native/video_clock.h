#ifndef LIANWALL_VIDEO_CLOCK_H
#define LIANWALL_VIDEO_CLOCK_H
#include <math.h>
#include <stdbool.h>

/* All `now` arguments use one monotonic clock, never wall time or GUI queries. */
struct lw_video_clock {
    bool started, paused;
    double epoch, paused_pts;
};
static inline bool lw_clock_start(struct lw_video_clock *clock, double now, double pts) {
    if (!isfinite(now) || !isfinite(pts) || pts < 0)
        return false;
    *clock = (struct lw_video_clock){.started = true, .epoch = now - pts};
    return true;
}
static inline double lw_clock_position(const struct lw_video_clock *clock, double now) {
    if (!clock->started)
        return 0;
    return clock->paused ? clock->paused_pts : fmax(0, now - clock->epoch);
}
static inline void lw_clock_pause(struct lw_video_clock *clock, double now) {
    if (clock->started && !clock->paused) {
        clock->paused_pts = lw_clock_position(clock, now);
        clock->paused = true;
    }
}
static inline void lw_clock_resume(struct lw_video_clock *clock, double now) {
    if (clock->started && clock->paused) {
        clock->epoch = now - clock->paused_pts;
        clock->paused = false;
    }
}
#endif

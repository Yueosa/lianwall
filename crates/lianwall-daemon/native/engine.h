#ifndef LIANWALL_ENGINE_H
#define LIANWALL_ENGINE_H
#include <stdint.h>
struct lw_engine;
struct lw_options {
    int fit, duration_ms, fps;
};
struct lw_stats {
    uint64_t decodes, uploads, cache_hits, frames;
    uint64_t video_frames, video_dropped, video_loops;
    uint64_t video_hardware_frames, video_software_frames, video_fallbacks;
};
struct lw_engine *lw_create(int wake_fd);
int lw_show(struct lw_engine *, const char *path, const char *output, struct lw_options);
/* Experimental software video entry. Same GPU owner/context as images. */
int lw_show_video(struct lw_engine *, const char *path, const char *output, int fit);
/* decode: 0 software, 1 required Vulkan, 2 first-frame software fallback. */
int lw_show_video_mode(struct lw_engine *, const char *path, const char *output, int fit,
                       int decode);
/* Same decoder policy, with GPU crossfade to a live incoming video. */
int lw_show_video_transition(struct lw_engine *, const char *path, const char *output,
                             struct lw_options, int decode);
int lw_decoder_kind(struct lw_engine *); /* 0 none, 1 software, 2 Vulkan */
const char *lw_fallback_reason(struct lw_engine *);
const char *lw_decode_diagnostic(struct lw_engine *);
int lw_pump(struct lw_engine *, int timeout_ms);
void lw_hide(struct lw_engine *);
void lw_destroy(struct lw_engine *);
const char *lw_error(struct lw_engine *);
struct lw_stats lw_statistics(struct lw_engine *);
#endif

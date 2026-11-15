#ifndef LIANWALL_VIDEO_QUEUE_H
#define LIANWALL_VIDEO_QUEUE_H
#include "video.h"
#include <stddef.h>

#define LW_VIDEO_AGAIN 2
struct lw_video_queue;
struct lw_video_queue_options {
    unsigned max_frames; /* 1..16; bounds queued frames, not codec DPB or consumer references */
    size_t max_bytes; /* 1..256 MiB; charges CPU buffers/side data plus hardware provider memory */
    uint64_t loops;   /* 0 repeats until cancelled */
};
struct lw_video_queue_stats {
    unsigned frames, peak_frames;
    size_t bytes, peak_bytes;
    uint64_t waits, produced;
    uint64_t hardware_frames, software_frames, fallbacks;
};
struct lw_video_queue_hardware {
    AVBufferRef *device;
    lw_video_hardware_check check;
    lw_video_hardware_prepare prepare;
    void *opaque; /* must be owned by retained device, not by its AVBufferRef wrapper */
    size_t (*frame_bytes)(const AVFrame *);
    bool fallback; /* only before publishing any frame, never on cancel/timeout */
};
/* Starts a dedicated decoder owner. Path/options are copied. No GPU or Wayland calls. */
struct lw_video_queue *lw_video_queue_create(const char *path,
                                             struct lw_video_queue_options options);
struct lw_video_queue *lw_video_queue_create_hardware(const char *path,
                                                      struct lw_video_queue_options options,
                                                      struct lw_video_queue_hardware hardware);
void lw_video_queue_diagnostic(struct lw_video_queue *queue, char *output, size_t size);
/* Nonblocking single-consumer pop. Initialize output to zero, clear before reuse.
 * FRAME transfers ownership; AGAIN means poll fd; EOF/errors follow queued frames.
 * Cancellation takes precedence over queued frames. timeline_pts spans natural loops. */
int lw_video_queue_pop(struct lw_video_queue *queue, struct lw_video_frame *output,
                       double *timeline_pts);
/* Borrowed poll-only fd: do not read/close it. pop drains/rearms the notification. */
int lw_video_queue_fd(const struct lw_video_queue *queue);
void lw_video_queue_stats(struct lw_video_queue *queue, struct lw_video_queue_stats *stats);
void lw_video_queue_error(struct lw_video_queue *queue, char *output, size_t size);
/* Thread-safe cancellation (not signal-safe). Wakes producer and poll consumer. */
void lw_video_queue_cancel(struct lw_video_queue *queue);
/* Cancel, join, release buffered frames and close fds. No concurrent calls at destruction.
 * Join uses decoder cooperative cancellation; third-party blocking I/O is not hard bounded. */
void lw_video_queue_destroy(struct lw_video_queue *queue);
#endif

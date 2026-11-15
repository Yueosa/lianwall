#ifndef LIANWALL_VIDEO_H
#define LIANWALL_VIDEO_H
#include <libavutil/frame.h>
#include <stdbool.h>
#include <stdint.h>

struct lw_video;
struct AVCodecContext;
typedef bool (*lw_video_hardware_check)(void *opaque, const struct AVCodecContext *context);
typedef int (*lw_video_hardware_prepare)(void *opaque, struct AVCodecContext *context);
typedef bool (*lw_video_cancel)(void *opaque);
enum lw_video_result {
    LW_VIDEO_EOF = 0,
    LW_VIDEO_FRAME = 1,
    LW_VIDEO_ERROR = -1,
    LW_VIDEO_CANCELLED = -2,
    LW_VIDEO_TIMEOUT = -3,
};
struct lw_video_frame {
    AVFrame *frame;       /* caller owns this reference; no CPU RGBA conversion */
    double pts, duration; /* seconds relative to the first frame of this loop */
    bool pts_synthesized, duration_estimated;
    uint64_t loop, index;
};
/* Each context has one owner thread; cancel must be safe to call on that thread. */
struct lw_video *lw_video_create(lw_video_cancel cancel, void *opaque);
/* Experimental strict Vulkan decoder mode; call before open. Retains device.
 * Unsupported codec/profile/device fails; never silently falls back to CPU.
 * Mandatory capability check runs on the decoder owner before accepting the
 * hardware format. Its opaque data must outlive the decoder. */
int lw_video_set_hardware(struct lw_video *video, AVBufferRef *device,
                          lw_video_hardware_check check, void *opaque);
int lw_video_set_hardware_frames(struct lw_video *video, lw_video_hardware_prepare prepare);
int lw_video_open(struct lw_video *video, const char *local_path);
/* Initialize output to zero; release each frame before reusing the output slot. */
int lw_video_next(struct lw_video *video, struct lw_video_frame *output);
/* Drain to EOF before rewind; resets decoder buffers, PTS origin and loop index. */
int lw_video_rewind(struct lw_video *video);
void lw_video_frame_clear(struct lw_video_frame *frame);
const char *lw_video_error(const struct lw_video *video);
void lw_video_destroy(struct lw_video *video);
#endif

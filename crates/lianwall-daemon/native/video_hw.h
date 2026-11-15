#ifndef LIANWALL_VIDEO_HW_H
#define LIANWALL_VIDEO_HW_H
#include "video.h"
#include <libavutil/hwcontext.h>
#include <libplacebo/vulkan.h>
/* GPU owner creates the device with video queues before creating any decoder.
 * Optional extensions/features preserve rendering on GPUs without hardware decode. */
pl_vulkan lw_video_device_create(pl_log log, VkInstance instance, VkSurfaceKHR surface);
/* Borrows vk/instance; every decoder, queued frame and mapping must be released
 * before destroying their GPU. Bridge mutexes are owned by the AVHWDeviceContext. */
AVBufferRef *lw_video_hw_create(pl_vulkan vk, pl_vk_inst instance);
bool lw_video_hw_check(void *context, const struct AVCodecContext *codec);
int lw_video_hw_prepare(void *context, struct AVCodecContext *codec);
/* Actual distinct VkDeviceMemory sizes for this output frame. DPB/session memory
 * is separate. Rejects invalid frames or declared initial pools over 24 surfaces;
 * a dynamic pool (initial size zero) is not a fixed 24-surface allocation quota. */
size_t lw_video_hw_bytes(const AVFrame *frame);
#endif

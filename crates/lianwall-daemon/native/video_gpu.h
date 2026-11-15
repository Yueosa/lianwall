#ifndef LIANWALL_VIDEO_GPU_H
#define LIANWALL_VIDEO_GPU_H
#include <libavutil/frame.h>
#include <libplacebo/renderer.h>
struct lw_video_gpu;
/* All operations, including destruction, stay on the GPU owner thread. */
struct lw_video_gpu *lw_video_gpu_create(pl_gpu gpu);
/* Explicitly accepts Vulkan frames from this GPU's exact VkDevice, or CPU fallback. */
struct lw_video_gpu *lw_video_gpu_create_hardware(pl_gpu gpu);
/* Default mappings accept software frames. Hardware-enabled mappings also accept
 * same-device Vulkan frames. Each mapping retains its own frame reference. */
int lw_video_gpu_map(struct lw_video_gpu *mapping, const AVFrame *frame);
int lw_video_gpu_render(struct lw_video_gpu *mapping, pl_renderer renderer,
                        const struct pl_frame *target, bool fit);
/* Borrowed until next map/clear/destroy. SAR is represented by the separate aspect getter. */
const struct pl_frame *lw_video_gpu_frame(const struct lw_video_gpu *mapping);
double lw_video_gpu_aspect(const struct lw_video_gpu *mapping);
const char *lw_video_gpu_error(const struct lw_video_gpu *mapping);
void lw_video_gpu_clear(struct lw_video_gpu *mapping);
void lw_video_gpu_destroy(struct lw_video_gpu *mapping);
#endif

/* Capability probe only. Does not import queues with flags this libplacebo cannot express. */
#include <libavutil/hwcontext.h>
#include <libavutil/hwcontext_vulkan.h>
#include <libplacebo/config.h>
#include <stdio.h>
int main(void) {
    AVBufferRef *device = NULL;
    int result = av_hwdevice_ctx_create(&device, AV_HWDEVICE_TYPE_VULKAN, "0", NULL, 0);
    if (result < 0) {
        fprintf(stderr, "Vulkan device creation failed: %d\n", result);
        return 1;
    }
    AVHWDeviceContext *context = (void *)device->data;
    AVVulkanDeviceContext *vk = context->hwctx;
    int decode = 0;
    for (int i = 0; i < vk->nb_qf; i++)
        if (vk->qf[i].flags & VK_QUEUE_VIDEO_DECODE_BIT_KHR)
            decode++;
    printf("{\"libplacebo_api\":%d,\"queue_flags\":%u,\"decode_queue_families\":%d,"
           "\"import_queue_flags_representable\":%s}\n",
           PL_API_VER, (unsigned)vk->queue_flags, decode,
           PL_API_VER >= 365 || vk->queue_flags == 0 ? "true" : "false");
    av_buffer_unref(&device);
    return 0;
}

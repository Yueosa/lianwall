#include "video_hw.h"
#include <assert.h>
#include <libavcodec/avcodec.h>
#include <libavutil/hwcontext_vulkan.h>
#include <libavutil/imgutils.h>
#include <libavutil/pixdesc.h>
#include <limits.h>
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
struct bridge {
    pl_vulkan vk;
    pthread_mutex_t extra[64];
};
static int shared_queue(struct bridge *b, uint32_t family, uint32_t index) {
    struct pl_vulkan_queue queues[] = {b->vk->queue_graphics, b->vk->queue_compute,
                                       b->vk->queue_transfer};
    for (unsigned i = 0; i < 3; i++)
        if (queues[i].count && queues[i].index == family && index < queues[i].count)
            return 1;
    return 0;
}
static void lock_queue(AVHWDeviceContext *context, uint32_t family, uint32_t index) {
    struct bridge *b = context->user_opaque;
    if (shared_queue(b, family, index))
        b->vk->lock_queue(b->vk, family, index);
    else {
        assert(family < 64 && index == 0);
        pthread_mutex_lock(&b->extra[family]);
    }
}
static void unlock_queue(AVHWDeviceContext *context, uint32_t family, uint32_t index) {
    struct bridge *b = context->user_opaque;
    if (shared_queue(b, family, index))
        b->vk->unlock_queue(b->vk, family, index);
    else {
        assert(family < 64 && index == 0);
        pthread_mutex_unlock(&b->extra[family]);
    }
}
/* Reject unsupported profiles before FFmpeg 9's hardware frame-parameter
 * bootstrap. Its failed initialization path can retain an internal reference. */
bool lw_video_hw_check(void *opaque, const AVCodecContext *codec) {
    struct bridge *b = ((AVHWDeviceContext *)opaque)->user_opaque;
    const AVPixFmtDescriptor *pixel = av_pix_fmt_desc_get(codec->sw_pix_fmt);
    if (!pixel || pixel->nb_components > 3 || codec->coded_width < 1 || codec->coded_height < 1 ||
        codec->coded_width > 8192 || codec->coded_height > 8192 ||
        (int64_t)codec->coded_width * codec->coded_height > 16 * 1024 * 1024 ||
        (pixel->flags & AV_PIX_FMT_FLAG_RGB))
        return false;
    VkVideoComponentBitDepthFlagsKHR depth =
        pixel->comp[0].depth == 8    ? VK_VIDEO_COMPONENT_BIT_DEPTH_8_BIT_KHR
        : pixel->comp[0].depth == 10 ? VK_VIDEO_COMPONENT_BIT_DEPTH_10_BIT_KHR
        : pixel->comp[0].depth == 12 ? VK_VIDEO_COMPONENT_BIT_DEPTH_12_BIT_KHR
                                     : 0;
    VkVideoChromaSubsamplingFlagsKHR chroma =
        pixel->nb_components == 1 ? VK_VIDEO_CHROMA_SUBSAMPLING_MONOCHROME_BIT_KHR
        : pixel->log2_chroma_w == 1 && pixel->log2_chroma_h == 1
            ? VK_VIDEO_CHROMA_SUBSAMPLING_420_BIT_KHR
        : pixel->log2_chroma_w == 1 && pixel->log2_chroma_h == 0
            ? VK_VIDEO_CHROMA_SUBSAMPLING_422_BIT_KHR
        : pixel->log2_chroma_w == 0 && pixel->log2_chroma_h == 0
            ? VK_VIDEO_CHROMA_SUBSAMPLING_444_BIT_KHR
            : 0;
    if (!depth || !chroma)
        return false;
    VkVideoDecodeH264ProfileInfoKHR h264 = {
        .sType = VK_STRUCTURE_TYPE_VIDEO_DECODE_H264_PROFILE_INFO_KHR,
        .stdProfileIdc = (StdVideoH264ProfileIdc)(codec->profile & ~(AV_PROFILE_H264_CONSTRAINED |
                                                                     AV_PROFILE_H264_INTRA)),
        .pictureLayout = VK_VIDEO_DECODE_H264_PICTURE_LAYOUT_PROGRESSIVE_KHR};
    VkVideoDecodeH265ProfileInfoKHR h265 = {
        .sType = VK_STRUCTURE_TYPE_VIDEO_DECODE_H265_PROFILE_INFO_KHR,
        .stdProfileIdc = (StdVideoH265ProfileIdc)codec->profile};
    VkVideoDecodeVP9ProfileInfoKHR vp9 = {.sType =
                                              VK_STRUCTURE_TYPE_VIDEO_DECODE_VP9_PROFILE_INFO_KHR,
                                          .stdProfile = (StdVideoVP9Profile)codec->profile};
    VkVideoDecodeAV1ProfileInfoKHR av1 = {.sType =
                                              VK_STRUCTURE_TYPE_VIDEO_DECODE_AV1_PROFILE_INFO_KHR,
                                          .stdProfile = (StdVideoAV1Profile)codec->profile,
                                          .filmGrainSupport = VK_TRUE};
    VkVideoProfileInfoKHR profile = {.sType = VK_STRUCTURE_TYPE_VIDEO_PROFILE_INFO_KHR,
                                     .chromaSubsampling = chroma,
                                     .lumaBitDepth = depth,
                                     .chromaBitDepth = depth};
    VkVideoDecodeH264CapabilitiesKHR h264_caps = {
        .sType = VK_STRUCTURE_TYPE_VIDEO_DECODE_H264_CAPABILITIES_KHR};
    VkVideoDecodeH265CapabilitiesKHR h265_caps = {
        .sType = VK_STRUCTURE_TYPE_VIDEO_DECODE_H265_CAPABILITIES_KHR};
    VkVideoDecodeVP9CapabilitiesKHR vp9_caps = {
        .sType = VK_STRUCTURE_TYPE_VIDEO_DECODE_VP9_CAPABILITIES_KHR};
    VkVideoDecodeAV1CapabilitiesKHR av1_caps = {
        .sType = VK_STRUCTURE_TYPE_VIDEO_DECODE_AV1_CAPABILITIES_KHR};
    VkVideoDecodeCapabilitiesKHR decode = {.sType =
                                               VK_STRUCTURE_TYPE_VIDEO_DECODE_CAPABILITIES_KHR};
    const char *extension = NULL;
    switch (codec->codec_id) {
    case AV_CODEC_ID_H264:
        if (h264.stdProfileIdc != 66 && h264.stdProfileIdc != 77 && h264.stdProfileIdc != 100 &&
            h264.stdProfileIdc != 110 && h264.stdProfileIdc != 122 && h264.stdProfileIdc != 244)
            return false;
        extension = "VK_KHR_video_decode_h264";
        if (codec->field_order != AV_FIELD_UNKNOWN && codec->field_order != AV_FIELD_PROGRESSIVE)
            return false; /* interlaced profile negotiation is not validated by this probe */
        profile.videoCodecOperation = VK_VIDEO_CODEC_OPERATION_DECODE_H264_BIT_KHR;
        profile.pNext = &h264;
        decode.pNext = &h264_caps;
        break;
    case AV_CODEC_ID_HEVC:
        if (codec->profile < 1 || codec->profile > 3)
            return false;
        extension = "VK_KHR_video_decode_h265";
        profile.videoCodecOperation = VK_VIDEO_CODEC_OPERATION_DECODE_H265_BIT_KHR;
        profile.pNext = &h265;
        decode.pNext = &h265_caps;
        break;
    case AV_CODEC_ID_VP9:
        if (codec->profile < 0 || codec->profile > 3)
            return false;
        extension = "VK_KHR_video_decode_vp9";
        profile.videoCodecOperation = VK_VIDEO_CODEC_OPERATION_DECODE_VP9_BIT_KHR;
        profile.pNext = &vp9;
        decode.pNext = &vp9_caps;
        break;
    case AV_CODEC_ID_AV1:
        if (codec->profile < 0 || codec->profile > 2)
            return false;
        extension = "VK_KHR_video_decode_av1";
        profile.videoCodecOperation = VK_VIDEO_CODEC_OPERATION_DECODE_AV1_BIT_KHR;
        profile.pNext = &av1;
        decode.pNext = &av1_caps;
        break;
    default:
        return false;
    }
    bool enabled = false;
    for (int i = 0; i < b->vk->num_extensions; i++)
        if (!strcmp(b->vk->extensions[i], extension))
            enabled = true;
    if (!enabled)
        return false;
    VkVideoCapabilitiesKHR caps = {.sType = VK_STRUCTURE_TYPE_VIDEO_CAPABILITIES_KHR,
                                   .pNext = &decode};
    PFN_vkGetPhysicalDeviceVideoCapabilitiesKHR query =
        (PFN_vkGetPhysicalDeviceVideoCapabilitiesKHR)b->vk->get_proc_addr(
            b->vk->instance, "vkGetPhysicalDeviceVideoCapabilitiesKHR");
    if (!query)
        return false;
    VkResult result = query(b->vk->phys_device, &profile, &caps);
    if (getenv("LIANWALL_HW_TRACE"))
        fprintf(stderr, "HW_PREFLIGHT codec=%d profile=%d format=%d depth=%u result=%d\n",
                codec->codec_id, codec->profile, codec->sw_pix_fmt, depth, result);
    if (getenv("LIANWALL_HW_TRACE") && result == VK_SUCCESS) {
        VkVideoProfileListInfoKHR list = {.sType = VK_STRUCTURE_TYPE_VIDEO_PROFILE_LIST_INFO_KHR,
                                          .profileCount = 1,
                                          .pProfiles = &profile};
        VkPhysicalDeviceVideoFormatInfoKHR info = {
            .sType = VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_VIDEO_FORMAT_INFO_KHR,
            .pNext = &list,
            .imageUsage = VK_IMAGE_USAGE_TRANSFER_SRC_BIT | VK_IMAGE_USAGE_TRANSFER_DST_BIT |
                          VK_IMAGE_USAGE_SAMPLED_BIT | VK_IMAGE_USAGE_STORAGE_BIT |
                          VK_IMAGE_USAGE_VIDEO_DECODE_DST_BIT_KHR |
                          VK_IMAGE_USAGE_VIDEO_DECODE_DPB_BIT_KHR};
        PFN_vkGetPhysicalDeviceVideoFormatPropertiesKHR formats =
            (PFN_vkGetPhysicalDeviceVideoFormatPropertiesKHR)b->vk->get_proc_addr(
                b->vk->instance, "vkGetPhysicalDeviceVideoFormatPropertiesKHR");
        for (int variant = 0; variant < 2; variant++) {
            if (variant)
                info.imageUsage = VK_IMAGE_USAGE_TRANSFER_SRC_BIT | VK_IMAGE_USAGE_SAMPLED_BIT |
                                  VK_IMAGE_USAGE_VIDEO_DECODE_DST_BIT_KHR |
                                  VK_IMAGE_USAGE_VIDEO_DECODE_DPB_BIT_KHR;
            uint32_t n = 0;
            VkResult status = formats ? formats(b->vk->phys_device, &info, &n, NULL)
                                      : VK_ERROR_EXTENSION_NOT_PRESENT;
            fprintf(stderr, "HW_FMT_COUNT usage=%u status=%d count=%u\n", info.imageUsage, status,
                    n);
            if (status == VK_SUCCESS && n <= 64) {
                VkVideoFormatPropertiesKHR properties[64] = {0};
                for (uint32_t i = 0; i < n; i++)
                    properties[i].sType = VK_STRUCTURE_TYPE_VIDEO_FORMAT_PROPERTIES_KHR;
                if (formats(b->vk->phys_device, &info, &n, properties) == VK_SUCCESS)
                    for (uint32_t i = 0; i < n; i++)
                        fprintf(stderr, "HW_FORMAT format=%d flags=%u tiling=%d usage=%u\n",
                                properties[i].format, properties[i].imageCreateFlags,
                                properties[i].imageTiling, properties[i].imageUsageFlags);
            }
        }
    }
    if (result != VK_SUCCESS)
        return false;
    int image_bytes =
        av_image_get_buffer_size(codec->sw_pix_fmt, codec->coded_width, codec->coded_height, 64);
    if (image_bytes <= 0 || caps.maxDpbSlots > 24 ||
        (uint64_t)image_bytes * (caps.maxDpbSlots + 8ULL) > 512ULL * 1024 * 1024)
        return false;
    return codec->coded_width >= (int)caps.minCodedExtent.width &&
           codec->coded_width <= (int)caps.maxCodedExtent.width &&
           codec->coded_height >= (int)caps.minCodedExtent.height &&
           codec->coded_height <= (int)caps.maxCodedExtent.height;
}

int lw_video_hw_prepare(void *opaque, AVCodecContext *codec) {
    struct bridge *b = ((AVHWDeviceContext *)opaque)->user_opaque;
    if (codec->hw_frames_ctx)
        return 0;
    AVBufferRef *reference = NULL;
    int result = avcodec_get_hw_frames_parameters(codec, codec->hw_device_ctx, AV_PIX_FMT_VULKAN,
                                                  &reference);
    if (result < 0)
        return result;
    AVHWFramesContext *frames = (void *)reference->data;
    AVVulkanFramesContext *hw = frames->hwctx;
    if (frames->initial_pool_size > 24) {
        av_buffer_unref(&reference);
        return AVERROR(ENOMEM);
    }
    /* Generic format STORAGE support is not sufficient for a video profile. */
    VkImageUsageFlags usage = (hw->usage & ~VK_IMAGE_USAGE_STORAGE_BIT) |
                              VK_IMAGE_USAGE_SAMPLED_BIT | VK_IMAGE_USAGE_TRANSFER_SRC_BIT |
                              VK_IMAGE_USAGE_TRANSFER_DST_BIT;
    PFN_vkGetPhysicalDeviceVideoFormatPropertiesKHR query =
        (PFN_vkGetPhysicalDeviceVideoFormatPropertiesKHR)b->vk->get_proc_addr(
            b->vk->instance, "vkGetPhysicalDeviceVideoFormatPropertiesKHR");
    VkPhysicalDeviceVideoFormatInfoKHR info = {
        .sType = VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_VIDEO_FORMAT_INFO_KHR,
        .pNext = hw->create_pnext,
        .imageUsage = usage};
    uint32_t count = 0;
    VkImageCreateFlags flags = 0;
    if (query && query(b->vk->phys_device, &info, &count, NULL) == VK_SUCCESS && count &&
        count <= 64) {
        VkVideoFormatPropertiesKHR properties[64] = {0};
        for (uint32_t i = 0; i < count; i++)
            properties[i].sType = VK_STRUCTURE_TYPE_VIDEO_FORMAT_PROPERTIES_KHR;
        if (query(b->vk->phys_device, &info, &count, properties) == VK_SUCCESS)
            for (uint32_t i = 0; i < count; i++)
                if (properties[i].format == hw->format[0] &&
                    properties[i].imageTiling == hw->tiling &&
                    properties[i].imageType == VK_IMAGE_TYPE_2D &&
                    (properties[i].imageUsageFlags & usage) == usage &&
                    (properties[i].imageCreateFlags & VK_IMAGE_CREATE_MUTABLE_FORMAT_BIT)) {
                    flags = properties[i].imageCreateFlags & (VK_IMAGE_CREATE_MUTABLE_FORMAT_BIT |
                                                              VK_IMAGE_CREATE_EXTENDED_USAGE_BIT);
                    break;
                }
    }
    if (!flags) {
        av_buffer_unref(&reference);
        return AVERROR(ENOSYS);
    }
    hw->img_flags = flags;
    hw->usage = usage;
    result = av_hwframe_ctx_init(reference);
    if (result < 0) {
        av_buffer_unref(&reference);
        return result;
    }
    if (getenv("LIANWALL_HW_TRACE"))
        fprintf(stderr, "HW_POOL flags=%u usage=%u size=%d\n", hw->img_flags, hw->usage,
                frames->initial_pool_size);
    codec->hw_frames_ctx = reference;
    return 0;
}
static void free_bridge(struct bridge *b) {
    for (unsigned i = 0; i < 64; i++)
        pthread_mutex_destroy(&b->extra[i]);
    free(b);
}
static void free_device(AVHWDeviceContext *device) { free_bridge(device->user_opaque); }
pl_vulkan lw_video_device_create(pl_log log, VkInstance instance, VkSurfaceKHR surface) {
    const char *extensions[] = {"VK_KHR_video_maintenance1", "VK_KHR_video_maintenance2",
                                "VK_KHR_video_queue",        "VK_KHR_video_decode_queue",
                                "VK_KHR_video_decode_h264",  "VK_KHR_video_decode_h265",
                                "VK_KHR_video_decode_av1",   "VK_KHR_video_decode_vp9"};
    VkPhysicalDeviceVideoMaintenance2FeaturesKHR maintenance2 = {
        .sType = VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_VIDEO_MAINTENANCE_2_FEATURES_KHR,
        .videoMaintenance2 = VK_TRUE,
        .pNext = pl_vulkan_recommended_features.pNext};
    VkPhysicalDeviceVideoMaintenance1FeaturesKHR maintenance1 = {
        .sType = VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_VIDEO_MAINTENANCE_1_FEATURES_KHR,
        .videoMaintenance1 = VK_TRUE,
        .pNext = &maintenance2};
    VkPhysicalDeviceVideoDecodeVP9FeaturesKHR vp9_features = {
        .sType = VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_VIDEO_DECODE_VP9_FEATURES_KHR,
        .videoDecodeVP9 = VK_TRUE,
        .pNext = &maintenance1};
    VkPhysicalDeviceFeatures2 features = pl_vulkan_recommended_features;
    features.pNext = &vp9_features;
    return pl_vulkan_create(
        log, pl_vulkan_params(.instance = instance, .surface = surface, .allow_software = false,
                              .extra_queues = VK_QUEUE_VIDEO_DECODE_BIT_KHR, .queue_count = 1,
                              .opt_extensions = extensions,
                              .num_opt_extensions = sizeof(extensions) / sizeof(*extensions),
                              .features = &features));
}
AVBufferRef *lw_video_hw_create(pl_vulkan vk, pl_vk_inst instance) {
    if (!vk || !instance || vk->api_version < VK_API_VERSION_1_3)
        return NULL;
    bool decode_enabled = false;
    for (int i = 0; i < vk->num_extensions; i++)
        if (!strcmp(vk->extensions[i], "VK_KHR_video_decode_queue"))
            decode_enabled = true;
    if (!decode_enabled)
        return NULL;
    struct bridge *bridge = calloc(1, sizeof(*bridge));
    if (!bridge)
        return NULL;
    bridge->vk = vk;
    for (unsigned i = 0; i < 64; i++) {
        if (pthread_mutex_init(&bridge->extra[i], NULL)) {
            for (unsigned j = 0; j < i; j++)
                pthread_mutex_destroy(&bridge->extra[j]);
            free(bridge);
            return NULL;
        }
    }
    AVBufferRef *device = av_hwdevice_ctx_alloc(AV_HWDEVICE_TYPE_VULKAN);
    if (!device) {
        free_bridge(bridge);
        return NULL;
    }
    AVHWDeviceContext *context = (void *)device->data;
    context->user_opaque = bridge;
    context->free = free_device;
    AVVulkanDeviceContext *hw = context->hwctx;
    hw->inst = vk->instance;
    hw->phys_dev = vk->phys_device;
    hw->act_dev = vk->device;
    hw->get_proc_addr = vk->get_proc_addr;
    hw->device_features = *vk->features;
    hw->enabled_inst_extensions = instance->extensions;
    hw->nb_enabled_inst_extensions = instance->num_extensions;
    hw->enabled_dev_extensions = vk->extensions;
    hw->nb_enabled_dev_extensions = vk->num_extensions;
#if PL_API_VER >= 365
    hw->queue_flags = vk->queue_graphics.flags;
    assert(!vk->queue_compute.count || vk->queue_compute.flags == hw->queue_flags);
    assert(!vk->queue_transfer.count || vk->queue_transfer.flags == hw->queue_flags);
#else
    hw->queue_flags = 0; /* This version creates ordinary queues, not FFmpeg's default flags=4. */
#endif
    hw->lock_queue = lock_queue;
    hw->unlock_queue = unlock_queue;
    uint32_t count = 0;
    vkGetPhysicalDeviceQueueFamilyProperties2(vk->phys_device, &count, NULL);
    if (count > 64) {
        av_buffer_unref(&device);
        return NULL;
    }
    VkQueueFamilyProperties2 properties[64] = {0};
    VkQueueFamilyVideoPropertiesKHR video[64] = {0};
    for (uint32_t i = 0; i < count; i++) {
        video[i].sType = VK_STRUCTURE_TYPE_QUEUE_FAMILY_VIDEO_PROPERTIES_KHR;
        properties[i].sType = VK_STRUCTURE_TYPE_QUEUE_FAMILY_PROPERTIES_2;
        properties[i].pNext = &video[i];
    }
    vkGetPhysicalDeviceQueueFamilyProperties2(vk->phys_device, &count, properties);
    for (uint32_t i = 0; i < count; i++) {
        unsigned num = 0;
        struct pl_vulkan_queue queues[] = {vk->queue_graphics, vk->queue_compute,
                                           vk->queue_transfer};
        for (unsigned j = 0; j < 3; j++)
            if (queues[j].count && queues[j].index == i)
                num = queues[j].count;
        if (properties[i].queueFamilyProperties.queueFlags & VK_QUEUE_VIDEO_DECODE_BIT_KHR)
            num = 1;
        if (num)
            hw->qf[hw->nb_qf++] =
                (AVVulkanDeviceQueueFamily){.idx = (int)i,
                                            .num = (int)num,
                                            .flags = properties[i].queueFamilyProperties.queueFlags,
                                            .video_caps = video[i].videoCodecOperations};
    }
    if (av_hwdevice_ctx_init(device) < 0) {
        av_buffer_unref(&device);
        return NULL;
    }
    return device;
}
size_t lw_video_hw_bytes(const AVFrame *frame) {
    if (!frame || frame->format != AV_PIX_FMT_VULKAN || !frame->hw_frames_ctx || !frame->data[0])
        return SIZE_MAX;
    const AVHWFramesContext *pool = (void *)frame->hw_frames_ctx->data;
    if (pool->initial_pool_size > 24)
        return SIZE_MAX;
    const AVVkFrame *vk = (void *)frame->data[0];
    size_t bytes = 0;
    for (unsigned i = 0; i < AV_NUM_DATA_POINTERS; i++) {
        if (!vk->mem[i])
            continue;
        bool duplicate = false;
        for (unsigned j = 0; j < i; j++)
            if (vk->mem[j] == vk->mem[i])
                duplicate = true;
        if (duplicate)
            continue;
        if (!vk->size[i] || vk->size[i] > SIZE_MAX - bytes)
            return SIZE_MAX;
        bytes += vk->size[i];
    }
    return bytes ? bytes : SIZE_MAX;
}

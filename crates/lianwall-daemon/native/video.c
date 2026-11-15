#define _POSIX_C_SOURCE 200809L
#include "video.h"
#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/avstring.h>
#include <libavutil/error.h>
#include <libavutil/hwcontext.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <time.h>

struct lw_video {
    AVFormatContext *format;
    AVCodecContext *codec;
    AVPacket *packet;
    AVBufferRef *hardware;
    lw_video_hardware_check hardware_check;
    lw_video_hardware_prepare hardware_prepare;
    void *hardware_opaque;
    bool hardware_rejected;
    lw_video_cancel cancel;
    void *opaque;
    int stream;
    bool draining, eof, failed, first_packet_seen;
    int64_t first_decode_ts;
    double deadline, origin_pts, last_pts, next_pts, nominal_duration;
    int64_t origin_ts;
    AVRational time_base;
    uint64_t loop, index;
    char error[512];
};
static double monotonic(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return ts.tv_sec + ts.tv_nsec / 1e9;
}
static bool cancelled(struct lw_video *v) { return v->cancel && v->cancel(v->opaque); }
static int interrupted(void *opaque) {
    struct lw_video *v = opaque;
    return cancelled(v) || (v->deadline && monotonic() >= v->deadline);
}
static int failure(struct lw_video *v, const char *operation, int code) {
    char reason[AV_ERROR_MAX_STRING_SIZE] = "";
    av_strerror(code, reason, sizeof(reason));
    snprintf(v->error, sizeof(v->error), "%s: %s", operation, reason);
    v->failed = true;
    return LW_VIDEO_ERROR;
}
static int check_interrupt(struct lw_video *v) {
    if (cancelled(v)) {
        snprintf(v->error, sizeof(v->error), "Video operation cancelled");
        return LW_VIDEO_CANCELLED;
    }
    if (v->deadline && monotonic() >= v->deadline) {
        snprintf(v->error, sizeof(v->error), "Video operation timed out");
        return LW_VIDEO_TIMEOUT;
    }
    return 0;
}
static int io_failure(struct lw_video *v, const char *operation, int code) {
    int stop = check_interrupt(v);
    return stop ? stop : failure(v, operation, code);
}
static void reset_timeline(struct lw_video *v) {
    v->origin_ts = AV_NOPTS_VALUE;
    v->origin_pts = 0;
    v->last_pts = -1;
    v->next_pts = 0;
    v->index = 0;
}
struct lw_video *lw_video_create(lw_video_cancel cancel, void *opaque) {
    struct lw_video *v = calloc(1, sizeof(*v));
    if (v) {
        v->cancel = cancel;
        v->opaque = opaque;
        v->stream = -1;
        v->first_decode_ts = AV_NOPTS_VALUE;
        reset_timeline(v);
    }
    return v;
}
int lw_video_set_hardware(struct lw_video *v, AVBufferRef *device, lw_video_hardware_check check,
                          void *opaque) {
    if (!v || v->format || !device || !check ||
        ((AVHWDeviceContext *)device->data)->type != AV_HWDEVICE_TYPE_VULKAN)
        return LW_VIDEO_ERROR;
    AVBufferRef *reference = av_buffer_ref(device);
    if (!reference)
        return LW_VIDEO_ERROR;
    av_buffer_unref(&v->hardware);
    v->hardware = reference;
    v->hardware_check = check;
    v->hardware_opaque = opaque;
    return 0;
}
int lw_video_set_hardware_frames(struct lw_video *v, lw_video_hardware_prepare prepare) {
    if (!v || v->format || !v->hardware || !prepare)
        return LW_VIDEO_ERROR;
    v->hardware_prepare = prepare;
    return 0;
}
static enum AVPixelFormat hardware_format(AVCodecContext *context,
                                          const enum AVPixelFormat *formats) {
    struct lw_video *v = context->opaque;
    bool offered = false;
    for (const enum AVPixelFormat *f = formats; *f != AV_PIX_FMT_NONE; f++)
        if (*f == AV_PIX_FMT_VULKAN)
            offered = true;
    if (!offered) {
        v->hardware_rejected = true;
        return AV_PIX_FMT_NONE;
    }
    if (!v->hardware_check(v->hardware_opaque, context)) {
        v->hardware_rejected = true;
        return AV_PIX_FMT_NONE;
    }
    if (v->hardware_prepare && v->hardware_prepare(v->hardware_opaque, context) < 0) {
        v->hardware_rejected = true;
        return AV_PIX_FMT_NONE;
    }
    return AV_PIX_FMT_VULKAN;
}
int lw_video_open(struct lw_video *v, const char *path) {
    if (!v || !path)
        return LW_VIDEO_ERROR;
    if (v->format || v->codec)
        return failure(v, "Video context is already opened", AVERROR(EINVAL));
    v->deadline = monotonic() + 5;
    int stop = check_interrupt(v);
    if (stop)
        return stop;
    struct stat st;
    if (stat(path, &st) || !S_ISREG(st.st_mode))
        return failure(v, "Video must be a local regular file", AVERROR(EINVAL));
    v->format = avformat_alloc_context();
    v->packet = av_packet_alloc();
    if (!v->format || !v->packet)
        return failure(v, "Allocate video context", AVERROR(ENOMEM));
    v->format->interrupt_callback = (AVIOInterruptCB){interrupted, v};
    v->format->max_streams = 16;
    v->format->probesize = 8 * 1024 * 1024;
    v->format->max_analyze_duration = 1000000;
    AVDictionary *options = NULL;
    av_dict_set(&options, "protocol_whitelist", "file", 0);
    int rc = avformat_open_input(&v->format, path, NULL, &options);
    av_dict_free(&options);
    if (rc < 0)
        return io_failure(v, "Open video", rc);
    AVDictionary *probe[16] = {0};
    for (int i = 0; i < 16; i++) {
        av_dict_set(&probe[i], "max_pixels", "16777216", 0);
        av_dict_set(&probe[i], "threads", "1", 0);
    }
    rc = avformat_find_stream_info(v->format, probe);
    for (int i = 0; i < 16; i++)
        av_dict_free(&probe[i]);
    if (rc < 0)
        return io_failure(v, "Probe video", rc);
    const AVCodec *decoder = NULL;
    v->stream = av_find_best_stream(v->format, AVMEDIA_TYPE_VIDEO, -1, -1, &decoder, 0);
    if (v->stream < 0)
        return failure(v, "Find video stream", v->stream);
    for (unsigned i = 0; i < v->format->nb_streams; i++)
        if ((int)i != v->stream)
            v->format->streams[i]->discard = AVDISCARD_ALL;
    if (v->hardware) {
        decoder = avcodec_find_decoder_by_name(
            avcodec_get_name(v->format->streams[v->stream]->codecpar->codec_id));
        bool supported = false;
        if (decoder)
            for (int i = 0;; i++) {
                const AVCodecHWConfig *config = avcodec_get_hw_config(decoder, i);
                if (!config)
                    break;
                if (config->device_type == AV_HWDEVICE_TYPE_VULKAN &&
                    config->pix_fmt == AV_PIX_FMT_VULKAN &&
                    (config->methods & AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX))
                    supported = true;
            }
        if (!supported)
            return failure(v, "Codec has no Vulkan device decoder", AVERROR(ENOSYS));
    }
    v->codec = avcodec_alloc_context3(decoder);
    if (!v->codec)
        return failure(v, "Allocate decoder", AVERROR(ENOMEM));
    AVStream *stream = v->format->streams[v->stream];
    v->time_base = stream->time_base;
    if (v->time_base.num <= 0 || v->time_base.den <= 0)
        return failure(v, "Invalid video time base", AVERROR_INVALIDDATA);
    rc = avcodec_parameters_to_context(v->codec, stream->codecpar);
    if (rc < 0)
        return failure(v, "Configure decoder", rc);
    v->codec->pkt_timebase = v->time_base;
    v->codec->max_pixels = 16 * 1024 * 1024;
    v->codec->thread_count = 1;
    v->codec->err_recognition = AV_EF_CRCCHECK | AV_EF_CAREFUL;
    if (v->hardware) {
        v->codec->hw_device_ctx = av_buffer_ref(v->hardware);
        if (!v->codec->hw_device_ctx)
            return failure(v, "Retain hardware device", AVERROR(ENOMEM));
        v->codec->get_format = hardware_format;
        v->codec->opaque = v;
        v->codec->extra_hw_frames = 6;
    }
    rc = avcodec_open2(v->codec, decoder, NULL);
    if (rc < 0)
        return failure(v, "Open decoder", rc);
    AVRational rate = av_guess_frame_rate(v->format, stream, NULL);
    v->nominal_duration = rate.num > 0 && rate.den > 0 ? av_q2d(av_inv_q(rate)) : 1.0 / 30;
    if (!isfinite(v->nominal_duration) || v->nominal_duration <= 0)
        return failure(v, "Invalid nominal frame duration", AVERROR_INVALIDDATA);
    v->deadline = 0;
    return 0;
}
/* Copy stream-level display metadata into each independently owned frame.
 * Frame-specific metadata wins, and queued frames must not borrow AVStream memory. */
static int copy_stream_properties(struct lw_video *v, AVFrame *frame) {
    AVStream *stream = v->format->streams[v->stream];
    AVRational sar = av_guess_sample_aspect_ratio(v->format, stream, frame);
    frame->sample_aspect_ratio = sar.num > 0 && sar.den > 0 ? sar : (AVRational){1, 1};
    const struct {
        enum AVPacketSideDataType packet;
        enum AVFrameSideDataType frame;
    } mappings[] = {
        {AV_PKT_DATA_DISPLAYMATRIX, AV_FRAME_DATA_DISPLAYMATRIX},
        {AV_PKT_DATA_ICC_PROFILE, AV_FRAME_DATA_ICC_PROFILE},
        {AV_PKT_DATA_MASTERING_DISPLAY_METADATA, AV_FRAME_DATA_MASTERING_DISPLAY_METADATA},
        {AV_PKT_DATA_CONTENT_LIGHT_LEVEL, AV_FRAME_DATA_CONTENT_LIGHT_LEVEL},
    };
    for (unsigned i = 0; i < sizeof(mappings) / sizeof(*mappings); i++) {
        if (av_frame_get_side_data(frame, mappings[i].frame))
            continue;
        const AVPacketSideData *data =
            av_packet_side_data_get(stream->codecpar->coded_side_data,
                                    stream->codecpar->nb_coded_side_data, mappings[i].packet);
        if (!data)
            continue;
        AVFrameSideData *copy = av_frame_new_side_data(frame, mappings[i].frame, data->size);
        if (!copy)
            return failure(v, "Copy stream display metadata", AVERROR(ENOMEM));
        memcpy(copy->data, data->data, data->size);
    }
    return 0;
}
static int timestamp(struct lw_video *v, struct lw_video_frame *out) {
    AVFrame *frame = out->frame;
    int64_t raw = frame->best_effort_timestamp;
    if (raw == AV_NOPTS_VALUE)
        raw = frame->pts;
    out->duration_estimated = frame->duration <= 0;
    out->duration =
        frame->duration > 0 ? frame->duration * av_q2d(v->time_base) : v->nominal_duration;
    out->pts_synthesized = raw == AV_NOPTS_VALUE;
    if (out->pts_synthesized) {
        out->pts = v->next_pts;
    } else {
        if (v->origin_ts == AV_NOPTS_VALUE) {
            v->origin_ts = raw;
            v->origin_pts = v->next_pts;
        }
        // Subtract in integer time-base units first, preserving small deltas
        // even when a container starts at a very large timestamp.
        if (raw < v->origin_ts || (v->origin_ts < 0 && raw > INT64_MAX + v->origin_ts))
            return failure(v, "Invalid timestamp span", AVERROR_INVALIDDATA);
        out->pts = (raw - v->origin_ts) * av_q2d(v->time_base) + v->origin_pts;
    }
    if (!isfinite(out->pts) || !isfinite(out->duration) || out->pts < 0 || out->duration <= 0 ||
        (v->index && out->pts <= v->last_pts))
        return failure(v, "Non-increasing or invalid presentation timestamp", AVERROR_INVALIDDATA);
    out->loop = v->loop;
    out->index = v->index++;
    v->last_pts = out->pts;
    v->next_pts = out->pts + out->duration;
    return LW_VIDEO_FRAME;
}
int lw_video_next(struct lw_video *v, struct lw_video_frame *out) {
    if (!v || !out)
        return LW_VIDEO_ERROR;
    if (out->frame)
        return failure(v, "Release the previous output frame first", AVERROR(EINVAL));
    if (!v->codec || v->failed)
        return failure(v, "Decoder is not ready", AVERROR(EINVAL));
    v->deadline = monotonic() + 3;
    int stop = check_interrupt(v);
    if (stop)
        return stop;
    if (v->eof)
        return LW_VIDEO_EOF;
    AVFrame *frame = av_frame_alloc();
    if (!frame)
        return failure(v, "Allocate frame", AVERROR(ENOMEM));
    int result = LW_VIDEO_ERROR;
    for (;;) {
        stop = check_interrupt(v);
        if (stop) {
            result = stop;
            break;
        }
        int rc = avcodec_receive_frame(v->codec, frame);
        if (rc == 0) {
            if (frame->flags & AV_FRAME_FLAG_DISCARD) {
                av_frame_unref(frame);
                continue;
            }
            if (frame->width < 1 || frame->height < 1 || frame->width > 8192 ||
                frame->height > 8192 || (int64_t)frame->width * frame->height > 16 * 1024 * 1024 ||
                (frame->flags & AV_FRAME_FLAG_CORRUPT) || frame->decode_error_flags) {
                result = failure(v, "Corrupt or oversized decoded frame", AVERROR_INVALIDDATA);
                break;
            }
            if (copy_stream_properties(v, frame)) {
                result = LW_VIDEO_ERROR;
                break;
            }
            out->frame = frame;
            result = timestamp(v, out);
            if (result == LW_VIDEO_FRAME) {
                frame = NULL;
                break;
            }
            out->frame = NULL;
            break;
        }
        if (rc == AVERROR_EOF) {
            v->eof = true;
            result = LW_VIDEO_EOF;
            break;
        }
        if (rc != AVERROR(EAGAIN)) {
            result = failure(v, "Receive decoded frame", rc);
            break;
        }
        if (v->draining) {
            result = failure(v, "Decoder made no progress while draining", AVERROR_INVALIDDATA);
            break;
        }
        do {
            av_packet_unref(v->packet);
            rc = av_read_frame(v->format, v->packet);
            stop = check_interrupt(v);
            if (stop) {
                result = stop;
                goto done;
            }
        } while (rc >= 0 && v->packet->stream_index != v->stream);
        if (rc == AVERROR_EOF) {
            rc = avcodec_send_packet(v->codec, NULL);
            if (rc < 0 && rc != AVERROR_EOF) {
                result = failure(v, "Flush decoder at EOF", rc);
                break;
            }
            v->draining = true;
        } else if (rc < 0) {
            result = io_failure(v, "Read video packet", rc);
            break;
        } else {
            if (!v->first_packet_seen) {
                v->first_packet_seen = true;
                v->first_decode_ts =
                    v->packet->dts != AV_NOPTS_VALUE ? v->packet->dts : v->packet->pts;
            }
            // Only a real demux EOF may send the decoder's NULL flush packet.
            if (!v->packet->data && !v->packet->size) {
                if (v->packet->side_data_elems) {
                    result =
                        failure(v, "Unsupported empty configuration packet", AVERROR_INVALIDDATA);
                    break;
                }
                continue;
            }
            if (v->packet->size < 0 || v->packet->size > 64 * 1024 * 1024) {
                result = failure(v, "Video packet exceeds budget", AVERROR_INVALIDDATA);
                break;
            }
            rc = avcodec_send_packet(v->codec, v->packet);
            av_packet_unref(v->packet);
            if (rc < 0) {
                result = failure(v,
                                 v->hardware_rejected ? "Vulkan profile/format preflight rejected"
                                                      : "Send video packet",
                                 rc);
                break;
            }
        }
    }
done:
    av_frame_free(&frame);
    v->deadline = 0;
    return result;
}
int lw_video_rewind(struct lw_video *v) {
    if (!v || !v->codec || v->failed)
        return LW_VIDEO_ERROR;
    v->deadline = monotonic() + 3;
    int stop = check_interrupt(v);
    if (stop)
        return stop;
    AVStream *stream = v->format->streams[v->stream];
    // B-frame streams may begin decoding before their presentation start.
    // Rewind to the first input DTS, not the first displayed frame's PTS.
    int64_t target = v->first_decode_ts != AV_NOPTS_VALUE   ? v->first_decode_ts
                     : stream->start_time == AV_NOPTS_VALUE ? 0
                                                            : stream->start_time;
    int rc =
        avformat_seek_file(v->format, v->stream, INT64_MIN, target, target, AVSEEK_FLAG_BACKWARD);
    if (rc < 0)
        return io_failure(v, "Seek to loop start", rc);
    avcodec_flush_buffers(v->codec);
    av_packet_unref(v->packet);
    v->draining = false;
    v->eof = false;
    v->loop++;
    reset_timeline(v);
    v->deadline = 0;
    return 0;
}
void lw_video_frame_clear(struct lw_video_frame *out) {
    if (!out)
        return;
    av_frame_free(&out->frame);
    *out = (struct lw_video_frame){0};
}
const char *lw_video_error(const struct lw_video *v) {
    return v ? v->error : "Video allocation failed";
}
void lw_video_destroy(struct lw_video *v) {
    if (!v)
        return;
    av_packet_free(&v->packet);
    avcodec_free_context(&v->codec);
    avformat_close_input(&v->format);
    av_buffer_unref(&v->hardware);
    free(v);
}

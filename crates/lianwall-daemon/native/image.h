#ifndef LIANWALL_IMAGE_H
#define LIANWALL_IMAGE_H
#include <lcms2.h>
#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/pixdesc.h>
#include <libplacebo/renderer.h>
#include <libswscale/swscale.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>
#include <sys/stat.h>
#include <time.h>
struct image_data {
    int w, h;
    unsigned char *rgba;
};
/* Bounded local-file SDR experiment; normalizes EXIF orthogonal transforms and RGB ICC to sRGB. */
static double image_now(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return ts.tv_sec + ts.tv_nsec / 1e9;
}
static int expired(void *data) { return image_now() > *(double *)data; }
static int normalize(struct image_data *out, const AVFrame *frame) {
    AVFrameSideData *icc = av_frame_get_side_data(frame, AV_FRAME_DATA_ICC_PROFILE);
    if (icc) {
        cmsHPROFILE input = cmsOpenProfileFromMem(icc->data, icc->size),
                    output = cmsCreate_sRGBProfile();
        if (!input || !output || cmsGetColorSpace(input) != cmsSigRgbData) {
            if (input)
                cmsCloseProfile(input);
            if (output)
                cmsCloseProfile(output);
            return 0;
        }
        cmsHTRANSFORM t = cmsCreateTransform(input, TYPE_RGBA_8, output, TYPE_RGBA_8,
                                             INTENT_RELATIVE_COLORIMETRIC, cmsFLAGS_COPY_ALPHA);
        if (t)
            cmsDoTransform(t, out->rgba, out->rgba, (cmsUInt32Number)out->w * out->h);
        if (t)
            cmsDeleteTransform(t);
        cmsCloseProfile(input);
        cmsCloseProfile(output);
        if (!t)
            return 0;
    }
    AVFrameSideData *sd = av_frame_get_side_data(frame, AV_FRAME_DATA_DISPLAYMATRIX);
    if (sd) {
        if (sd->size < 9 * sizeof(int32_t))
            return 0;
        const int32_t *m = (const int32_t *)sd->data;
        int a = m[0] / 65536, b = m[1] / 65536, c = m[3] / 65536, d = m[4] / 65536;
        if (m[0] != a * 65536 || m[1] != b * 65536 || m[3] != c * 65536 || m[4] != d * 65536 ||
            m[2] || m[5] || m[8] != (1 << 30) || abs(a) + abs(c) != 1 || abs(b) + abs(d) != 1 ||
            abs(a) + abs(b) != 1 || abs(c) + abs(d) != 1)
            return 0;
        int w = abs(a) * out->w + abs(c) * out->h, h = abs(b) * out->w + abs(d) * out->h;
        int minx = (a < 0 ? a * (out->w - 1) : 0) + (c < 0 ? c * (out->h - 1) : 0),
            miny = (b < 0 ? b * (out->w - 1) : 0) + (d < 0 ? d * (out->h - 1) : 0);
        unsigned char *buf = malloc((size_t)w * h * 4);
        if (!buf)
            return 0;
        for (int y = 0; y < out->h; y++)
            for (int x = 0; x < out->w; x++)
                memcpy(buf + 4 * ((size_t)(b * x + d * y - miny) * w + a * x + c * y - minx),
                       out->rgba + 4 * ((size_t)y * out->w + x), 4);
        free(out->rgba);
        out->rgba = buf;
        out->w = w;
        out->h = h;
    }
    return 1;
}
static int decode_farbfeld(const char *path, struct image_data *out) {
    FILE *file = fopen(path, "rb");
    if (!file)
        return 0;
    unsigned char header[16];
    int ok = 0;
    if (fread(header, 1, 16, file) != 16 || memcmp(header, "farbfeld", 8))
        goto done;
    uint32_t w = (uint32_t)header[8] << 24 | (uint32_t)header[9] << 16 | (uint32_t)header[10] << 8 |
                 header[11];
    uint32_t h = (uint32_t)header[12] << 24 | (uint32_t)header[13] << 16 |
                 (uint32_t)header[14] << 8 | header[15];
    if (!w || !h || w > 8192 || h > 8192 || (uint64_t)w * h > 16 * 1024 * 1024)
        goto done;
    struct stat st;
    if (fstat(fileno(file), &st) || st.st_size != (int64_t)w * h * 8 + 16)
        goto done;
    out->rgba = malloc((size_t)w * h * 4);
    if (!out->rgba)
        goto done;
    unsigned char *row = malloc((size_t)w * 8);
    if (!row)
        goto done;
    for (uint32_t y = 0; y < h; y++) {
        if (fread(row, 1, (size_t)w * 8, file) != (size_t)w * 8) {
            free(row);
            goto done;
        }
        for (size_t x = 0; x < (size_t)w * 4; x++)
            out->rgba[(size_t)y * w * 4 + x] = ((row[x * 2] * 256 + row[x * 2 + 1]) + 128) / 257;
    }
    free(row);
    out->w = w;
    out->h = h;
    ok = 1;
done:
    fclose(file);
    if (!ok) {
        free(out->rgba);
        *out = (struct image_data){0};
    }
    return ok;
}
static int decode_image(const char *path, struct image_data *out) {
    struct stat st;
    if (stat(path, &st) || !S_ISREG(st.st_mode) || st.st_size > 128 * 1024 * 1024)
        return 0;
    const char *extension = strrchr(path, '.');
    if (extension && !strcasecmp(extension, ".farbfeld"))
        return decode_farbfeld(path, out);
    double deadline = image_now() + 3;
    AVFormatContext *f = avformat_alloc_context();
    if (!f)
        return 0;
    f->interrupt_callback = (AVIOInterruptCB){expired, &deadline};
    f->max_streams = 4;
    f->probesize = 1024 * 1024;
    f->max_analyze_duration = 500000;
    AVDictionary *options = NULL;
    av_dict_set(&options, "protocol_whitelist", "file", 0);
    av_dict_set(&options, "pattern_type", "none", 0);
    AVCodecContext *c = NULL;
    AVFrame *converted = av_frame_alloc();
    AVFrame *frame = av_frame_alloc();
    AVPacket *p = av_packet_alloc();
    struct SwsContext *s = NULL;
    int ok = 0;
    if (!converted || !frame || !p || avformat_open_input(&f, path, NULL, &options) < 0)
        goto done;
    // Still-image demuxers normally identify the codec at open. Avoid decoding
    // a large image twice just to obtain video timing/stream information.
    if (!f->nb_streams || f->streams[0]->codecpar->codec_id == AV_CODEC_ID_NONE) {
        AVDictionary **probe = av_calloc(f->nb_streams, sizeof(*probe));
        if (!probe)
            goto done;
        for (unsigned i = 0; i < f->nb_streams; i++) {
            av_dict_set(&probe[i], "max_pixels", "16777216", 0);
            av_dict_set(&probe[i], "threads", "1", 0);
        }
        unsigned streams = f->nb_streams;
        int probed = avformat_find_stream_info(f, probe);
        for (unsigned i = 0; i < streams; i++)
            av_dict_free(&probe[i]);
        av_free(probe);
        if (probed < 0)
            goto done;
    }
    int index = av_find_best_stream(f, AVMEDIA_TYPE_VIDEO, -1, -1, NULL, 0);
    if (index < 0)
        goto done;
    switch (f->streams[index]->codecpar->codec_id) {
    case AV_CODEC_ID_MJPEG:
    case AV_CODEC_ID_PNG:
    case AV_CODEC_ID_APNG:
    case AV_CODEC_ID_GIF:
    case AV_CODEC_ID_BMP:
    case AV_CODEC_ID_WEBP:
#if LIBAVCODEC_VERSION_MAJOR >= 63
    case AV_CODEC_ID_WEBP_ANIM:
#endif
    case AV_CODEC_ID_TIFF:
    case AV_CODEC_ID_TARGA:
    case AV_CODEC_ID_PPM:
    case AV_CODEC_ID_PGM:
    case AV_CODEC_ID_PBM:
    case AV_CODEC_ID_PAM:
        break;
    default:
        goto done;
    }
    const AVCodec *codec = avcodec_find_decoder(f->streams[index]->codecpar->codec_id);
    if (!codec)
        goto done;
    c = avcodec_alloc_context3(codec);
    if (c) {
        c->max_pixels = 16 * 1024 * 1024;
        c->thread_count = 1;
        c->err_recognition = AV_EF_CRCCHECK | AV_EF_CAREFUL;
    }
    if (!c || avcodec_parameters_to_context(c, f->streams[index]->codecpar) < 0 ||
        avcodec_open2(c, codec, NULL) < 0)
        goto done;
    for (int packets = 0;; packets++) {
        if (packets >= 256 || expired(&deadline))
            goto done;
        int r = av_read_frame(f, p);
        if (r < 0) {
            if (r != AVERROR_EOF)
                goto done;
            if (avcodec_send_packet(c, NULL) < 0)
                goto done;
        } else {
            if (p->stream_index != index) {
                av_packet_unref(p);
                continue;
            }
            int sent = avcodec_send_packet(c, p);
            av_packet_unref(p);
            if (sent < 0)
                goto done;
        }
        int got = avcodec_receive_frame(c, frame);
        if (got == 0)
            break;
        if (got != AVERROR(EAGAIN) || r < 0)
            goto done;
    }
    if ((frame->flags & AV_FRAME_FLAG_CORRUPT) || frame->decode_error_flags)
        goto done;
    if (frame->width < 1 || frame->height < 1 || frame->width > 8192 || frame->height > 8192)
        goto done;
    if ((int64_t)frame->width * frame->height > 16 * 1024 * 1024 ||
        frame->color_trc == AVCOL_TRC_SMPTE2084 || frame->color_trc == AVCOL_TRC_ARIB_STD_B67)
        goto done;
    if (!av_frame_get_side_data(frame, AV_FRAME_DATA_ICC_PROFILE) &&
        ((frame->color_primaries != AVCOL_PRI_UNSPECIFIED &&
          frame->color_primaries != AVCOL_PRI_BT709) ||
         (frame->color_trc != AVCOL_TRC_UNSPECIFIED && frame->color_trc != AVCOL_TRC_IEC61966_2_1)))
        goto done;
    out->w = frame->width;
    out->h = frame->height;
    out->rgba = malloc((size_t)out->w * out->h * 4);
    if (!out->rgba)
        goto done;
    // Packed RGBA needs no conversion or second aligned output allocation.
    if (frame->format == AV_PIX_FMT_RGBA) {
        for (int row = 0; row < out->h; row++)
            memcpy(out->rgba + (size_t)row * out->w * 4,
                   frame->data[0] + (ptrdiff_t)row * frame->linesize[0], (size_t)out->w * 4);
        ok = normalize(out, frame);
        goto done;
    }
    s = sws_getContext(out->w, out->h, frame->format, out->w, out->h, AV_PIX_FMT_RGBA, SWS_BILINEAR,
                       NULL, NULL, NULL);
    if (!s)
        goto done;
    const AVPixFmtDescriptor *desc = av_pix_fmt_desc_get(frame->format);
    if (!(desc->flags & (AV_PIX_FMT_FLAG_RGB | AV_PIX_FMT_FLAG_PAL)) &&
        frame->colorspace != AVCOL_SPC_RGB) {
        int cs = frame->colorspace == AVCOL_SPC_BT709 ? SWS_CS_ITU709 : SWS_CS_ITU601;
        if (frame->colorspace != AVCOL_SPC_BT709 && frame->colorspace != AVCOL_SPC_BT470BG &&
            frame->colorspace != AVCOL_SPC_SMPTE170M && frame->colorspace != AVCOL_SPC_UNSPECIFIED)
            goto done;
        const int *table = sws_getCoefficients(cs);
        if (sws_setColorspaceDetails(s, table, frame->color_range == AVCOL_RANGE_JPEG, table, 1, 0,
                                     1 << 16, 1 << 16) < 0)
            goto done;
    }
    converted->format = AV_PIX_FMT_RGBA;
    converted->width = out->w;
    converted->height = out->h;
    if (av_frame_get_buffer(converted, 64) < 0)
        goto done;
    if (sws_scale(s, (const uint8_t *const *)frame->data, frame->linesize, 0, out->h,
                  converted->data, converted->linesize) != out->h)
        goto done;
    for (int row = 0; row < out->h; row++)
        memcpy(out->rgba + (size_t)row * out->w * 4,
               converted->data[0] + (size_t)row * converted->linesize[0], (size_t)out->w * 4);
    ok = normalize(out, frame);
done:
    av_frame_free(&converted);
    av_dict_free(&options);
    sws_freeContext(s);
    av_packet_free(&p);
    av_frame_free(&frame);
    avcodec_free_context(&c);
    avformat_close_input(&f);
    if (!ok) {
        free(out->rgba);
        *out = (struct image_data){0};
    }
    return ok;
}
static struct pl_frame rgb_frame(pl_tex texture) {
    return (struct pl_frame){
        .num_planes = 1,
        .planes = {{.texture = texture, .components = 4, .component_mapping = {0, 1, 2, 3}}},
        .repr = {.sys = PL_COLOR_SYSTEM_RGB,
                 .levels = PL_COLOR_LEVELS_FULL,
                 .alpha = PL_ALPHA_INDEPENDENT},
        .color = pl_color_space_srgb,
        .crop = {0, 0, texture->params.w, texture->params.h}};
}
static void layout(struct pl_frame *src, struct pl_frame *dst, int fit) {
    float sw = src->crop.x1 - src->crop.x0, sh = src->crop.y1 - src->crop.y0,
          dw = dst->crop.x1 - dst->crop.x0, dh = dst->crop.y1 - dst->crop.y0;
    if (fit) {
        float scale = fminf(dw / sw, dh / sh), w = sw * scale, h = sh * scale;
        dst->crop.x0 += (dw - w) / 2;
        dst->crop.x1 = dst->crop.x0 + w;
        dst->crop.y0 += (dh - h) / 2;
        dst->crop.y1 = dst->crop.y0 + h;
    } else {
        float scale = fmaxf(dw / sw, dh / sh), w = dw / scale, h = dh / scale;
        src->crop.x0 += (sw - w) / 2;
        src->crop.x1 = src->crop.x0 + w;
        src->crop.y0 += (sh - h) / 2;
        src->crop.y1 = src->crop.y0 + h;
    }
}
static struct pl_render_params render_params(void) {
    struct pl_render_params p = pl_render_default_params;
    p.border = PL_CLEAR_COLOR;
    p.background = PL_CLEAR_COLOR;
    p.background_transparency = 0;
    return p;
}

#endif

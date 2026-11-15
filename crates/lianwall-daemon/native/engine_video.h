/* Private engine implementation, included after the image renderer. All GPU
 * operations stay on the existing owner. Decoder/device policy stays in queue. */
struct video_session {
    struct lw_video_queue *queue;
    struct lw_video_gpu *mapping;
    struct lw_video_frame next;
    struct lw_video_clock clock;
    double pts;
    uint64_t loop;
    int ready, fit, hardware, unavailable;
};
static void free_video(struct lw_engine *e, struct video_session *v) {
    if (!v)
        return;
    lw_video_queue_destroy(v->queue);
    lw_video_frame_clear(&v->next);
    if (e->vk)
        pl_gpu_finish(e->vk->gpu);
    lw_video_gpu_destroy(v->mapping);
    free(v);
}
static void stop_video(struct lw_engine *e) {
    free_video(e, e->video);
    e->video = NULL;
}
static int scale_video(struct lw_engine *e, struct video_session *v, int index, int w, int h) {
    pl_tex *texture = &e->video_scaled[index];
    if (!*texture || (*texture)->params.w != w || (*texture)->params.h != h) {
        pl_tex_destroy(e->vk->gpu, texture);
        *texture = pl_tex_create(e->vk->gpu,
                                 pl_tex_params(.w = w, .h = h,
                                               .format = pl_find_named_fmt(e->vk->gpu, "rgba8"),
                                               .sampleable = true, .renderable = true));
    }
    if (!*texture)
        return fail(e, "Cannot allocate video transition target");
    struct pl_frame target = rgb_frame(*texture);
    return lw_video_gpu_render(v->mapping, e->renderer, &target, v->fit)
               ? fail(e, lw_video_gpu_error(v->mapping))
               : 0;
}
static int render_video(struct lw_engine *e, struct video_session *v) {
    int w = (e->width * e->scale + 119) / 120, h = (e->height * e->scale + 119) / 120;
    if (w < 1 || h < 1 || w > 8192 || h > 8192 || (int64_t)w * h > 16 * 1024 * 1024)
        return fail(e, "Output exceeds native video pixel budget");
    wp_viewport_set_destination(e->viewport, e->width, e->height);
    if (!pl_swapchain_resize(e->swap, &w, &h))
        return fail(e, "Cannot resize video swapchain");
    struct pl_swapchain_frame swap;
    if (!pl_swapchain_start_frame(e->swap, &swap))
        return fail(e, "Cannot acquire video frame");
    struct pl_frame target;
    pl_frame_from_swapchain(&target, &swap);
    if (lw_video_gpu_render(v->mapping, e->renderer, &target, v->fit))
        return fail(e, lw_video_gpu_error(v->mapping));
    clear_callbacks(e);
    e->callback = wl_surface_frame(e->surface);
    wl_callback_add_listener(e->callback, &frame_listener, e);
    e->feedback = wp_presentation_feedback(e->presentation, e->surface);
    wp_presentation_feedback_add_listener(e->feedback, &feedback_listener, e);
    if (!pl_swapchain_submit_frame(e->swap))
        return fail(e, "Cannot submit video frame");
    pl_swapchain_swap_buffers(e->swap);
    e->stats.frames++;
    e->stats.video_frames++;
    if (v->hardware)
        e->stats.video_hardware_frames++;
    else
        e->stats.video_software_frames++;
    e->last_submit = image_now();
    e->dirty = 0;
    return e->gpu_errors ? fail(e, "GPU reported a video error") : 0;
}
static int fetch_video(struct lw_engine *e, struct video_session *v) {
    v->ready = lw_video_queue_pop(v->queue, &v->next, &v->pts);
    if (v->ready < 0) {
        lw_video_queue_error(v->queue, e->error, sizeof(e->error));
        return -1;
    }
    return 0;
}
static int advance_video(struct lw_engine *e, struct video_session *v, double now) {
    if (!v)
        return 0;
    if (v->ready == LW_VIDEO_AGAIN && fetch_video(e, v))
        return -1;
    double time = lw_clock_position(&v->clock, now);
    if (v->ready == LW_VIDEO_FRAME && v->pts <= time) {
        struct lw_video_frame selected = v->next;
        v->next = (struct lw_video_frame){0};
        if (fetch_video(e, v)) {
            lw_video_frame_clear(&selected);
            return -1;
        }
        while (v->ready == LW_VIDEO_FRAME && v->pts <= lw_clock_position(&v->clock, now)) {
            e->stats.video_dropped++;
            lw_video_frame_clear(&selected);
            selected = v->next;
            v->next = (struct lw_video_frame){0};
            if (fetch_video(e, v)) {
                lw_video_frame_clear(&selected);
                return -1;
            }
        }
        e->stats.video_loops += selected.loop - v->loop;
        v->loop = selected.loop;
        int result = lw_video_gpu_map(v->mapping, selected.frame);
        lw_video_frame_clear(&selected);
        if (result)
            return fail(e, lw_video_gpu_error(v->mapping));
        return 1;
    }
    return 0;
}
static int video_step(struct lw_engine *e) {
    if (e->callback)
        return 0;
    int changed = advance_video(e, e->video, image_now());
    if (changed < 0)
        return -1;
    return changed || e->dirty ? render_video(e, e->video) : 0;
}
static int pump_video(struct lw_engine *e, int timeout) {
    int r = events(e, 0);
    if (r > 0)
        return r;
    if (r < 0) {
        fail(e, "Native video Wayland output or connection lost");
        goto fatal;
    }
    if (video_step(e))
        goto fatal;
    struct video_session *v = e->video;
    int wait = timeout < 0 ? 100 : timeout;
    if (!e->callback && v->ready == LW_VIDEO_FRAME) {
        double due = (v->pts - lw_clock_position(&v->clock, image_now())) * 1000;
        wait = (int)fmin(wait, fmax(1, due));
    }
    struct pollfd fds[3] = {
        {.fd = wl_display_get_fd(e->display), .events = POLLIN},
        {.fd = e->wake_fd, .events = POLLIN},
        {.fd = v->ready == LW_VIDEO_AGAIN ? lw_video_queue_fd(v->queue) : -1, .events = POLLIN},
    };
    if (poll(fds, 3, wait) < 0 && errno != EINTR)
        goto fatal;
    r = events(e, 0);
    if (r > 0)
        return r;
    if (r < 0) {
        fail(e, "Native video Wayland output or connection lost");
        goto fatal;
    }
    if (video_step(e))
        goto fatal;
    return 0;
fatal:
    e->fatal = 1;
    if (!*e->error)
        fail(e, "Native video output, decoder or GPU failed; restart daemon");
    stop_video(e);
    destroy_surface(e);
    return -1;
}
int lw_decoder_kind(struct lw_engine *e) { return e->video ? (e->video->hardware ? 2 : 1) : 0; }
const char *lw_fallback_reason(struct lw_engine *e) { return e->fallback_reason; }
const char *lw_decode_diagnostic(struct lw_engine *e) { return e->decode_reason; }
int lw_show_video(struct lw_engine *e, const char *path, const char *output, int fit) {
    return lw_show_video_mode(e, path, output, fit, 0);
}
static int initialize_video(struct lw_engine *e, const char *output) {
    if (!initialize(e, output))
        return 0;
    if (!e->compositor || !e->shell || !e->viewporter || !e->fractional_manager ||
        !e->presentation || (e->display && wl_display_get_error(e->display)) || e->gpu_errors)
        e->fatal = 1;
    return -1;
}
int lw_show_video_mode(struct lw_engine *e, const char *path, const char *output, int fit,
                       int decode) {
    return lw_show_video_transition(e, path, output, (struct lw_options){.fit = fit, .fps = 60},
                                    decode);
}
int lw_show_video_transition(struct lw_engine *e, const char *path, const char *output,
                             struct lw_options options, int decode) {
    int fit = options.fit;
    if (options.duration_ms < 0 || options.duration_ms > 2000 || options.fps < 1 ||
        options.fps > 120)
        return fail(e, "Invalid video transition options");
    if (decode < 0 || decode > 2)
        return fail(e, "Invalid native decoder policy");
    if (fit < 0 || fit > 1)
        return fail(e, "Invalid native video layout; select fit or crop");
    if (!*output || !strcmp(output, "*") || strchr(output, ',') ||
        strlen(output) >= sizeof(e->output_name))
        return fail(e, "Native video needs one named output: set video_engine.display in Engine > Target output (for example HDMI-A-1); empty, * and multiple outputs are unsupported");
    if (e->fatal)
        return fail(e, "Native context failed; restart daemon");
    struct video_session *v = calloc(1, sizeof(*v));
    if (!v)
        return fail(e, "Cannot allocate video source");
    v->fit = fit;
    v->ready = LW_VIDEO_AGAIN;
    struct lw_video_queue_options budget = {
        .max_frames = 4, .max_bytes = 64 * 1024 * 1024, .loops = 0};
    if (decode) {
        if (initialize_video(e, output))
            goto rejected;
        if (!e->hardware)
            e->hardware = lw_video_hw_create(e->vk, e->instance);
        if (e->hardware) {
            v->queue = lw_video_queue_create_hardware(
                path, budget,
                (struct lw_video_queue_hardware){.device = e->hardware,
                                                 .check = lw_video_hw_check,
                                                 .prepare = lw_video_hw_prepare,
                                                 .opaque = e->hardware->data,
                                                 .frame_bytes = lw_video_hw_bytes,
                                                 .fallback = decode == 2});
        } else if (decode == 1) {
            fail(e, "Vulkan video device is unavailable; choose auto/software or mpvpaper");
            goto rejected;
        } else
            v->unavailable = 1;
    }
    if (!v->queue && (!decode || v->unavailable))
        v->queue = lw_video_queue_create(path, budget);
    if (!v->queue) {
        fail(e, "Cannot create video decode queue");
        goto rejected;
    }
    double deadline = image_now() + 8;
    while (v->ready == LW_VIDEO_AGAIN && image_now() < deadline) {
        if (fetch_video(e, v))
            goto rejected;
        if (v->ready != LW_VIDEO_AGAIN)
            break;
        int r = lw_pump(e, 10);
        if (r) {
            fail(e, r > 0 ? "Video preparation cancelled" : "Previous native source failed");
            goto rejected;
        }
    }
    if (v->ready != LW_VIDEO_FRAME) {
        fail(e, "Video first frame unavailable or preparation timed out");
        goto rejected;
    }
    struct pollfd wake = {.fd = e->wake_fd, .events = POLLIN};
    if (poll(&wake, 1, 0) > 0 && wake.revents) {
        fail(e, "Video preparation cancelled");
        goto rejected;
    }
    if (initialize_video(e, output))
        goto rejected;
    v->hardware = v->next.frame->format == AV_PIX_FMT_VULKAN;
    v->mapping =
        decode ? lw_video_gpu_create_hardware(e->vk->gpu) : lw_video_gpu_create(e->vk->gpu);
    if (!v->mapping || lw_video_gpu_map(v->mapping, v->next.frame)) {
        fail(e, v->mapping ? lw_video_gpu_error(v->mapping) : "Cannot allocate video mapping");
        if (e->gpu_errors)
            goto fatal;
        goto rejected;
    }
    lw_video_frame_clear(&v->next);
    v->ready = LW_VIDEO_AGAIN;
    int animated = e->active && options.duration_ms > 0;
    if (animated) {
        if (!lw_clock_start(&v->clock, image_now(), 0)) {
            fail(e, "Cannot start candidate video clock");
            goto rejected;
        }
        int result = transition(e, -1, v, options);
        if (result < 0)
            goto fatal;
        if (result > 0) {
            if (result == 1)
                fail(e, "Video transition cancelled");
            goto rollback;
        }
        if (advance_video(e, v, image_now()) < 0)
            goto rollback;
    }
    e->first_presented = 0;
    e->first_discarded = 0;
    e->first_wait = 1;
    if (render_video(e, v))
        goto fatal;
    pl_gpu_finish(e->vk->gpu);
    deadline = image_now() + 3;
    while (!e->first_presented && !e->first_discarded && image_now() < deadline) {
        int r = events(e, 50);
        if (r < 0)
            goto fatal;
        if (r > 0) {
            fail(e, "Video first presentation cancelled");
            goto rollback;
        }
    }
    if (!isfinite(e->first_presented) || !e->first_presented || e->first_discarded ||
        (!animated && !lw_clock_start(&v->clock, e->first_presented, 0))) {
        fail(e, "Video first frame was not presented");
        goto rollback;
    }
    e->first_wait = 0;
    stop_video(e);
    e->video = v;
    struct lw_video_queue_stats stats;
    lw_video_queue_stats(v->queue, &stats);
    e->stats.video_fallbacks += stats.fallbacks + v->unavailable;
    char diagnostic[512];
    lw_video_queue_diagnostic(v->queue, diagnostic, sizeof(diagnostic));
    snprintf(e->decode_reason, sizeof(e->decode_reason), "%s%s%.400s",
             v->hardware ? "Vulkan hardware decode" : "Software decode",
             v->unavailable || stats.fallbacks ? "; fallback: " : "",
             v->unavailable ? "Vulkan device unavailable" : diagnostic);
    snprintf(e->fallback_reason, sizeof(e->fallback_reason), "%s",
             v->unavailable    ? "Vulkan device unavailable"
             : stats.fallbacks ? diagnostic
                               : "");
    e->current = -1;
    e->active = 1;
    clear_transition(e);
    return 0;
rollback:
    e->first_wait = 0;
    if (e->video) {
        if (render_video(e, e->video))
            goto fatal;
    } else if (e->active && e->current >= 0) {
        if (render(e, -1, e->current, 1, e->options.fit))
            goto fatal;
    } else
        destroy_surface(e);
rejected:
    clear_transition(e);
    e->first_wait = 0;
    free_video(e, v);
    if (!e->active)
        destroy_surface(e);
    return e->fatal ? -2 : -1;
fatal:
    clear_transition(e);
    e->first_wait = 0;
    e->fatal = 1;
    free_video(e, v);
    stop_video(e);
    destroy_surface(e);
    if (!*e->error)
        fail(e, "Native video first presentation failed; restart daemon");
    return -2;
}

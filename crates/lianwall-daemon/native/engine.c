#define VK_USE_PLATFORM_WAYLAND_KHR
#include "engine.h"
#include "fractional-client.h"
#include "image.h"
#include "layer-client.h"
#include "presentation-client.h"
#include "transition.h"
#include "video_clock.h"
#include "video_gpu.h"
#include "video_hw.h"
#include "video_queue.h"
#include "viewporter-client.h"
#include <errno.h>
#include <libplacebo/dispatch.h>
#include <libplacebo/log.h>
#include <libplacebo/shaders/custom.h>
#include <libplacebo/vulkan.h>
#include <limits.h>
#include <poll.h>
#include <vulkan/vulkan.h>
#include <wayland-client.h>

#define MAX_OUTPUTS 32
struct output {
    struct wl_output *object;
    uint32_t id;
    char name[128];
};
struct cached {
    char *path;
    struct stat stamp;
    pl_tex source, scaled;
    int sw, sh, fit;
};
struct lw_engine {
    int wake_fd, fatal, width, height, scale, dirty, active, current, gpu_errors, closed;
    double last_submit;
    char error[512], output_name[128];
    uint32_t presentation_clock_id;
    struct lw_stats stats;
    struct lw_options options;
    struct wl_display *display;
    struct wl_registry *registry;
    struct wl_compositor *compositor;
    struct zwlr_layer_shell_v1 *shell;
    struct wp_viewporter *viewporter;
    struct wp_fractional_scale_manager_v1 *fractional_manager;
    struct wp_presentation *presentation;
    struct wl_surface *surface;
    struct zwlr_layer_surface_v1 *layer;
    struct wp_viewport *viewport;
    struct wp_fractional_scale_v1 *fractional;
    struct wl_callback *callback;
    struct wp_presentation_feedback *feedback;
    struct output outputs[MAX_OUTPUTS];
    int selected;
    pl_log log;
    pl_vk_inst instance;
    pl_vulkan vk;
    AVBufferRef *hardware;
    char decode_reason[512], fallback_reason[512];
    VkSurfaceKHR vk_surface;
    pl_swapchain swap;
    pl_renderer renderer;
    pl_dispatch dispatch;
    pl_tex mixed, video_scaled[2];
    struct cached images[2];
    struct video_session *video;
    int first_wait, first_discarded;
    double first_presented;
};
static void stop_video(struct lw_engine *e);
static int advance_video(struct lw_engine *, struct video_session *, double);
static int scale_video(struct lw_engine *, struct video_session *, int, int, int);
static int render_video(struct lw_engine *, struct video_session *);
static int fail(struct lw_engine *e, const char *message) {
    snprintf(e->error, sizeof(e->error), "%s", message);
    return -1;
}
static void gpu_log(void *data, enum pl_log_level level, const char *message) {
    struct lw_engine *e = data;
    if (level <= PL_LOG_ERR) {
        e->gpu_errors++;
        snprintf(e->error, sizeof(e->error), "GPU: %.500s", message);
    }
}
static void geometry(void *data, struct wl_output *o, int32_t x, int32_t y, int32_t pw, int32_t ph,
                     int32_t sub, const char *make, const char *model, int32_t transform) {
    (void)data;
    (void)o;
    (void)x;
    (void)y;
    (void)pw;
    (void)ph;
    (void)sub;
    (void)make;
    (void)model;
    (void)transform;
}
static void mode(void *data, struct wl_output *o, uint32_t flags, int32_t w, int32_t h,
                 int32_t refresh) {
    (void)data;
    (void)o;
    (void)flags;
    (void)w;
    (void)h;
    (void)refresh;
}
static void done(void *data, struct wl_output *o) {
    (void)data;
    (void)o;
}
static void output_scale(void *data, struct wl_output *o, int32_t scale) {
    (void)data;
    (void)o;
    (void)scale;
}
static void output_name(void *data, struct wl_output *o, const char *name) {
    (void)o;
    snprintf(((struct output *)data)->name, 128, "%s", name);
}
static void description(void *data, struct wl_output *o, const char *name) {
    (void)data;
    (void)o;
    (void)name;
}
static const struct wl_output_listener output_listener = {geometry,     mode,        done,
                                                          output_scale, output_name, description};
static const struct wp_presentation_listener presentation_listener;
static void global(void *data, struct wl_registry *r, uint32_t id, const char *name,
                   uint32_t version) {
    struct lw_engine *e = data;
    if (!strcmp(name, "wl_compositor"))
        e->compositor =
            wl_registry_bind(r, id, &wl_compositor_interface, version < 4 ? version : 4);
    else if (!strcmp(name, "zwlr_layer_shell_v1"))
        e->shell = wl_registry_bind(r, id, &zwlr_layer_shell_v1_interface, 1);
    else if (!strcmp(name, "wp_viewporter"))
        e->viewporter = wl_registry_bind(r, id, &wp_viewporter_interface, 1);
    else if (!strcmp(name, "wp_fractional_scale_manager_v1"))
        e->fractional_manager =
            wl_registry_bind(r, id, &wp_fractional_scale_manager_v1_interface, 1);
    else if (!strcmp(name, "wp_presentation")) {
        e->presentation = wl_registry_bind(r, id, &wp_presentation_interface, 1);
        wp_presentation_add_listener(e->presentation, &presentation_listener, e);
    } else if (!strcmp(name, "wl_output") && version >= 4) {
        for (int i = 0; i < MAX_OUTPUTS; i++)
            if (!e->outputs[i].object) {
                struct output *out = &e->outputs[i];
                out->id = id;
                out->object = wl_registry_bind(r, id, &wl_output_interface, 4);
                wl_output_add_listener(out->object, &output_listener, out);
                break;
            }
    }
}
static void removed(void *data, struct wl_registry *r, uint32_t id) {
    (void)r;
    struct lw_engine *e = data;
    for (int i = 0; i < MAX_OUTPUTS; i++)
        if (e->outputs[i].id == id) {
            if (e->selected == i && e->surface)
                e->closed = 1;
            wl_output_release(e->outputs[i].object);
            e->outputs[i] = (struct output){0};
        }
}
static const struct wl_registry_listener registry_listener = {global, removed};
static void configured(void *data, struct zwlr_layer_surface_v1 *layer, uint32_t serial, uint32_t w,
                       uint32_t h) {
    struct lw_engine *e = data;
    zwlr_layer_surface_v1_ack_configure(layer, serial);
    if (w && h && w <= 8192 && h <= 8192) {
        e->width = w;
        e->height = h;
        e->dirty = 1;
    } else {
        e->closed = 1;
        fail(e, "Unsupported layer size");
    }
}
static void closed(void *data, struct zwlr_layer_surface_v1 *layer) {
    (void)layer;
    ((struct lw_engine *)data)->closed = 1;
}
static const struct zwlr_layer_surface_v1_listener layer_listener = {configured, closed};
static void preferred(void *data, struct wp_fractional_scale_v1 *object, uint32_t scale) {
    (void)object;
    struct lw_engine *e = data;
    if (scale >= 60 && scale <= 480) {
        e->scale = scale;
        e->dirty = 1;
    } else
        e->closed = 1;
}
static const struct wp_fractional_scale_v1_listener fractional_listener = {preferred};
static void clock_id(void *data, struct wp_presentation *p, uint32_t id) {
    (void)p;
    ((struct lw_engine *)data)->presentation_clock_id = id;
}
static const struct wp_presentation_listener presentation_listener = {clock_id};
static void sync_output(void *data, struct wp_presentation_feedback *f, struct wl_output *o) {
    (void)data;
    (void)f;
    (void)o;
}
static void presented(void *data, struct wp_presentation_feedback *f, uint32_t hi, uint32_t lo,
                      uint32_t ns, uint32_t refresh, uint32_t shi, uint32_t slo, uint32_t flags) {
    (void)refresh;
    (void)shi;
    (void)slo;
    (void)flags;
    struct lw_engine *e = data;
    if (e->first_wait) {
        struct timespec timestamp;
        double before = image_now();
        if (e->presentation_clock_id != UINT32_MAX &&
            !clock_gettime((clockid_t)e->presentation_clock_id, &timestamp)) {
            double after = image_now();
            double raw = (double)(((uint64_t)hi << 32) | lo) + ns / 1e9;
            e->first_presented =
                raw + (before + after) / 2 - (timestamp.tv_sec + timestamp.tv_nsec / 1e9);
        }
    }
    e->feedback = NULL;
    wp_presentation_feedback_destroy(f);
}
static void discarded(void *data, struct wp_presentation_feedback *f) {
    struct lw_engine *e = data;
    if (e->first_wait)
        e->first_discarded = 1;
    e->feedback = NULL;
    wp_presentation_feedback_destroy(f);
}
static const struct wp_presentation_feedback_listener feedback_listener = {sync_output, presented,
                                                                           discarded};
static void frame_done(void *data, struct wl_callback *cb, uint32_t timestamp) {
    (void)timestamp;
    ((struct lw_engine *)data)->callback = NULL;
    wl_callback_destroy(cb);
}
static const struct wl_callback_listener frame_listener = {frame_done};
static void clear_callbacks(struct lw_engine *e) {
    if (e->callback) {
        wl_callback_destroy(e->callback);
        e->callback = NULL;
    }
    if (e->feedback) {
        wp_presentation_feedback_destroy(e->feedback);
        e->feedback = NULL;
    }
}
static int events(struct lw_engine *e, int timeout) {
    if (!e->display) {
        struct pollfd fd = {.fd = e->wake_fd, .events = POLLIN};
        int n = poll(&fd, 1, timeout);
        return n < 0 && errno != EINTR ? -1 : n > 0 ? 1 : 0;
    }
    while (wl_display_prepare_read(e->display) != 0)
        if (wl_display_dispatch_pending(e->display) < 0)
            return -1;
    int flushed = wl_display_flush(e->display);
    if (flushed < 0 && errno != EAGAIN) {
        wl_display_cancel_read(e->display);
        return -1;
    }
    struct pollfd fd[2] = {
        {.fd = wl_display_get_fd(e->display), .events = POLLIN | (flushed < 0 ? POLLOUT : 0)},
        {.fd = e->wake_fd, .events = POLLIN}};
    int n = poll(fd, 2, timeout);
    if (n > 0 && (fd[0].revents & POLLIN)) {
        if (wl_display_read_events(e->display) < 0)
            return -1;
    } else
        wl_display_cancel_read(e->display);
    if (n < 0 && errno != EINTR)
        return -1;
    if (fd[0].revents & (POLLHUP | POLLERR | POLLNVAL))
        return -1;
    if (wl_display_dispatch_pending(e->display) < 0)
        return -1;
    if (e->closed)
        return -1;
    return fd[1].revents ? 1 : 0;
}
static void destroy_surface(struct lw_engine *e) {
    clear_callbacks(e);
    if (e->swap)
        pl_swapchain_destroy(&e->swap);
    if (e->vk_surface) {
        vkDestroySurfaceKHR(e->instance->instance, e->vk_surface, NULL);
        e->vk_surface = VK_NULL_HANDLE;
    }
    if (e->fractional) {
        wp_fractional_scale_v1_destroy(e->fractional);
        e->fractional = NULL;
    }
    if (e->viewport) {
        wp_viewport_destroy(e->viewport);
        e->viewport = NULL;
    }
    if (e->layer) {
        zwlr_layer_surface_v1_destroy(e->layer);
        e->layer = NULL;
    }
    if (e->surface) {
        wl_surface_destroy(e->surface);
        e->surface = NULL;
    }
    e->active = 0;
    e->dirty = 0;
    e->width = 0;
    e->height = 0;
}
static int initialize(struct lw_engine *e, const char *name) {
    if (e->fatal)
        return fail(e,
                    "Native engine failed; restart the daemon to recover its GPU/Wayland context");
    if (!e->display) {
        e->display = wl_display_connect(NULL);
        if (!e->display)
            return fail(e, "Cannot connect to Wayland");
        e->registry = wl_display_get_registry(e->display);
        wl_registry_add_listener(e->registry, &registry_listener, e);
        // A timed roundtrip: never wait indefinitely for a compositor.
        struct wl_callback *sync = wl_display_sync(e->display);
        wl_callback_add_listener(sync, &frame_listener, e);
        e->callback = sync;
        double deadline = image_now() + 3;
        while (e->callback && image_now() < deadline) {
            int r = events(e, 100);
            if (r)
                return fail(e, r > 0 ? "Cancelled" : "Wayland connection lost");
        }
        if (e->callback)
            return fail(e, "Wayland registry timed out");
        sync = wl_display_sync(e->display);
        wl_callback_add_listener(sync, &frame_listener, e);
        e->callback = sync;
        deadline = image_now() + 3;
        while (e->callback && image_now() < deadline) {
            int r = events(e, 100);
            if (r)
                return fail(e, r > 0 ? "Cancelled" : "Wayland connection lost");
        }
        if (e->callback)
            return fail(e, "Wayland output enumeration timed out");
        if (!e->compositor || !e->shell || !e->viewporter || !e->fractional_manager ||
            !e->presentation)
            return fail(e, "Native images require layer-shell, wl_output v4, viewporter, "
                           "fractional-scale and presentation-time");
    }
    if (e->surface && !strcmp(name, e->output_name))
        return 0;
    if (e->instance && strcmp(name, e->output_name))
        return fail(e, "Changing native output requires a daemon restart");
    destroy_surface(e);
    e->closed = 0;
    e->scale = 120;
    e->selected = -1;
    for (int i = 0; i < MAX_OUTPUTS; i++)
        if (e->outputs[i].object && !strcmp(e->outputs[i].name, name)) {
            e->selected = i;
            break;
        }
    if (e->selected < 0)
        return fail(e, "Configured native output is unavailable (specify exactly one output name)");
    snprintf(e->output_name, sizeof(e->output_name), "%s", name);
    e->surface = wl_compositor_create_surface(e->compositor);
    struct wl_region *empty = wl_compositor_create_region(e->compositor);
    wl_surface_set_input_region(e->surface, empty);
    wl_region_destroy(empty);
    e->viewport = wp_viewporter_get_viewport(e->viewporter, e->surface);
    e->fractional =
        wp_fractional_scale_manager_v1_get_fractional_scale(e->fractional_manager, e->surface);
    wp_fractional_scale_v1_add_listener(e->fractional, &fractional_listener, e);
    e->layer = zwlr_layer_shell_v1_get_layer_surface(
        e->shell, e->surface, e->outputs[e->selected].object, ZWLR_LAYER_SHELL_V1_LAYER_BACKGROUND,
        "lianwall-native");
    zwlr_layer_surface_v1_add_listener(e->layer, &layer_listener, e);
    zwlr_layer_surface_v1_set_anchor(e->layer, 15);
    zwlr_layer_surface_v1_set_size(e->layer, 0, 0);
    zwlr_layer_surface_v1_set_exclusive_zone(e->layer, -1);
    zwlr_layer_surface_v1_set_keyboard_interactivity(e->layer, 0);
    wl_surface_commit(e->surface);
    double deadline = image_now() + 3;
    while (!e->width && image_now() < deadline) {
        int r = events(e, 100);
        if (r)
            return fail(e, r > 0 ? "Cancelled" : "Wayland layer closed");
    }
    if (!e->width)
        return fail(e, "Layer configure timed out");
    if (!e->instance) {
        e->log = pl_log_create(
            PL_API_VER, pl_log_params(.log_cb = gpu_log, .log_priv = e, .log_level = PL_LOG_ERR));
        const char *extensions[] = {VK_KHR_SURFACE_EXTENSION_NAME,
                                    VK_KHR_WAYLAND_SURFACE_EXTENSION_NAME};
        e->instance = pl_vk_inst_create(
            e->log, pl_vk_inst_params(.extensions = extensions, .num_extensions = 2));
        if (!e->instance)
            return fail(e, "Cannot create Vulkan instance");
    }
    VkWaylandSurfaceCreateInfoKHR si = {.sType = VK_STRUCTURE_TYPE_WAYLAND_SURFACE_CREATE_INFO_KHR,
                                        .display = e->display,
                                        .surface = e->surface};
    if (vkCreateWaylandSurfaceKHR(e->instance->instance, &si, NULL, &e->vk_surface) != VK_SUCCESS)
        return fail(e, "Cannot create Vulkan Wayland surface");
    if (!e->vk) {
        e->vk = lw_video_device_create(e->log, e->instance->instance, e->vk_surface);
        if (!e->vk)
            return fail(e, "Cannot create hardware Vulkan device");
        e->renderer = pl_renderer_create(e->log, e->vk->gpu);
        e->dispatch = pl_dispatch_create(e->log, e->vk->gpu);
        if (!e->renderer || !e->dispatch)
            return fail(e, "Cannot create renderer");
    }
    e->swap = pl_vulkan_create_swapchain(
        e->vk,
        pl_vulkan_swapchain_params(.surface = e->vk_surface,
                                   .present_mode = VK_PRESENT_MODE_FIFO_KHR, .swapchain_depth = 2,
                                   .disable_10bit_sdr = true, .color_bits = 8));
    if (!e->swap)
        return fail(e, "Cannot create Vulkan swapchain");
    return 0;
}
static int same_stamp(const struct stat *a, const struct stat *b) {
    return a->st_dev == b->st_dev && a->st_ino == b->st_ino && a->st_size == b->st_size &&
           a->st_mtim.tv_sec == b->st_mtim.tv_sec && a->st_mtim.tv_nsec == b->st_mtim.tv_nsec &&
           a->st_ctim.tv_sec == b->st_ctim.tv_sec && a->st_ctim.tv_nsec == b->st_ctim.tv_nsec;
}
static void evict(struct lw_engine *e, int i) {
    struct cached *c = &e->images[i];
    free(c->path);
    if (e->vk) {
        pl_tex_destroy(e->vk->gpu, &c->source);
        pl_tex_destroy(e->vk->gpu, &c->scaled);
    }
    *c = (struct cached){0};
}
static int scale_image(struct lw_engine *e, int i, int w, int h, int fit) {
    struct cached *c = &e->images[i];
    if (c->scaled && c->sw == w && c->sh == h && c->fit == fit)
        return 0;
    pl_tex_destroy(e->vk->gpu, &c->scaled);
    pl_fmt fmt = pl_find_named_fmt(e->vk->gpu, "rgba8");
    c->scaled =
        pl_tex_create(e->vk->gpu, pl_tex_params(.w = w, .h = h, .format = fmt, .sampleable = true,
                                                .renderable = true, .blit_dst = true));
    if (!c->scaled)
        return fail(e, "Cannot allocate scaled image");
    struct pl_frame src = rgb_frame(c->source), dst = rgb_frame(c->scaled);
    layout(&src, &dst, fit);
    struct pl_render_params params = render_params();
    if (!pl_render_image(e->renderer, &src, &dst, &params))
        return fail(e, "Cannot scale image");
    c->sw = w;
    c->sh = h;
    c->fit = fit;
    return 0;
}
static int render_media(struct lw_engine *e, int old, int next, struct video_session *video,
                        float progress, int fit) {
    int w = (e->width * e->scale + 119) / 120, h = (e->height * e->scale + 119) / 120;
    if (w < 1 || h < 1 || w > 8192 || h > 8192 || (int64_t)w * h > 16 * 1024 * 1024)
        return fail(e, "Output exceeds native image pixel budget");
    wp_viewport_set_destination(e->viewport, e->width, e->height);
    if (!pl_swapchain_resize(e->swap, &w, &h))
        return fail(e, "Cannot resize swapchain");
    pl_tex source;
    if (video) {
        if (scale_video(e, video, 1, w, h))
            return -1;
        source = e->video_scaled[1];
    } else {
        if (scale_image(e, next, w, h, fit))
            return -1;
        source = e->images[next].scaled;
    }
    if ((old >= 0 || old == -2) && progress < 1) {
        pl_tex previous;
        if (old == -2) {
            if (scale_video(e, e->video, 0, w, h))
                return -1;
            previous = e->video_scaled[0];
        } else {
            if (scale_image(e, old, w, h, e->options.fit))
                return -1;
            previous = e->images[old].scaled;
        }
        if (!e->mixed || e->mixed->params.w != w || e->mixed->params.h != h) {
            pl_tex_destroy(e->vk->gpu, &e->mixed);
            e->mixed = pl_tex_create(e->vk->gpu,
                                     pl_tex_params(.w = w, .h = h,
                                                   .format = pl_find_named_fmt(e->vk->gpu, "rgba8"),
                                                   .sampleable = true, .renderable = true));
        }
        if (!e->mixed)
            return fail(e, "Cannot allocate transition target");
        if (!crossfade(e->dispatch, previous, source, e->mixed, progress))
            return fail(e, "Cannot render transition");
        source = e->mixed;
    }
    struct pl_swapchain_frame frame;
    if (!pl_swapchain_start_frame(e->swap, &frame))
        return fail(e, "Cannot acquire wallpaper frame");
    struct pl_frame src = rgb_frame(source), dst;
    pl_frame_from_swapchain(&dst, &frame);
    struct pl_render_params params = render_params();
    if (!pl_render_image(e->renderer, &src, &dst, &params))
        return fail(e, "Cannot render wallpaper frame");
    clear_callbacks(e);
    e->callback = wl_surface_frame(e->surface);
    wl_callback_add_listener(e->callback, &frame_listener, e);
    e->feedback = wp_presentation_feedback(e->presentation, e->surface);
    wp_presentation_feedback_add_listener(e->feedback, &feedback_listener, e);
    if (!pl_swapchain_submit_frame(e->swap))
        return fail(e, "Cannot submit wallpaper frame");
    pl_swapchain_swap_buffers(e->swap);
    e->stats.frames++;
    e->last_submit = image_now();
    e->dirty = 0;
    return e->gpu_errors ? fail(e, "GPU reported an error") : 0;
}
static int render(struct lw_engine *e, int old, int next, float progress, int fit) {
    return render_media(e, old, next, NULL, progress, fit);
}
static void clear_transition(struct lw_engine *e) {
    if (!e->vk)
        return;
    pl_tex_destroy(e->vk->gpu, &e->mixed);
    for (int i = 0; i < 2; i++)
        pl_tex_destroy(e->vk->gpu, &e->video_scaled[i]);
}
/* Old media remains owned until the final candidate presentation is accepted.
 * Both video clocks advance during the fade; no decoder-frame CPU readback. */
static int transition(struct lw_engine *e, int next, struct video_session *video,
                      struct lw_options options) {
    int old = e->active ? (e->video ? -2 : e->current) : -1;
    if (old == -1 || (!video && old == next) || !options.duration_ms)
        return 0;
    double start = image_now(), end = start + options.duration_ms / 1000.0, due = start;
    while (image_now() < end) {
        double now = image_now();
        if (now >= due && !e->callback) {
            if (e->video && advance_video(e, e->video, now) < 0)
                return -1;
            if (video && advance_video(e, video, now) < 0)
                return e->gpu_errors ? -1 : 2; /* Candidate failed; keep old source. */
            float progress = (now - start) / (end - start);
            progress = progress * progress * (3 - 2 * progress);
            if (render_media(e, old, next, video, progress, options.fit))
                return -1;
            due = now + 1.0 / options.fps;
        }
        int wait = e->callback ? 50 : (int)fmax(1, fmin(50, (due - image_now()) * 1000));
        int result = events(e, wait);
        if (result)
            return result;
        // Hidden surfaces can stop receiving frame callbacks; never wait forever.
        if (e->callback && image_now() - e->last_submit > .25)
            break;
    }
    return events(e, 0);
}
struct lw_engine *lw_create(int wake_fd) {
    struct lw_engine *e = calloc(1, sizeof(*e));
    if (e) {
        e->wake_fd = wake_fd;
        e->current = -1;
        e->selected = -1;
        e->presentation_clock_id = UINT32_MAX;
        e->scale = 120;
    }
    return e;
}
const char *lw_error(struct lw_engine *e) {
    return e ? e->error : "Cannot allocate native image engine";
}
struct lw_stats lw_statistics(struct lw_engine *e) { return e->stats; }
int lw_show(struct lw_engine *e, const char *path, const char *output, struct lw_options options) {
    if (options.fit < 0 || options.fit > 1 || options.duration_ms < 0 ||
        options.duration_ms > 2000 || options.fps < 1 || options.fps > 120)
        return fail(e, "Invalid image options");
    if (!output[0] || strchr(output, ',') || strlen(output) >= 128)
        return fail(e, "Experimental native images require exactly one image_engine.outputs name");
    struct stat st;
    if (stat(path, &st) || !S_ISREG(st.st_mode))
        return fail(e, "Image is not a local regular file");
    int target = -1;
    for (int i = 0; i < 2; i++)
        if (e->images[i].path && !strcmp(path, e->images[i].path) &&
            same_stamp(&st, &e->images[i].stamp)) {
            target = i;
            e->stats.cache_hits++;
            break;
        }
    if (target >= 0 && !e->video && target == e->current && e->active && !e->dirty &&
        e->options.fit == options.fit && !strcmp(output, e->output_name)) {
        e->options = options;
        return 0;
    }
    struct image_data decoded = {0};
    if (target < 0) {
        if (!decode_image(path, &decoded))
            return fail(e, "Image decoding failed: unsupported format/color metadata, corrupt "
                           "file, or resource limit");
        e->stats.decodes++;
        target = e->current == 0 ? 1 : 0;
    }
    // Cancellation may arrive while the bounded CPU decoder is running.
    // Never upload/commit that result merely because no animation was requested.
    struct pollfd wake = {.fd = e->wake_fd, .events = POLLIN};
    if (poll(&wake, 1, 0) > 0 && wake.revents) {
        free(decoded.rgba);
        return fail(e, "Image preparation cancelled");
    }
    if (initialize(e, output)) {
        free(decoded.rgba);
        if (!e->active)
            destroy_surface(e);
        if (!e->compositor || !e->shell || !e->viewporter || !e->fractional_manager ||
            !e->presentation || (e->display && wl_display_get_error(e->display)) || e->gpu_errors)
            e->fatal = 1;
        return e->fatal ? -2 : -1;
    }
    if (decoded.rgba) {
        evict(e, target);
        struct cached *c = &e->images[target];
        c->path = strdup(path);
        c->stamp = st;
        c->source = pl_tex_create(e->vk->gpu,
                                  pl_tex_params(.w = decoded.w, .h = decoded.h,
                                                .format = pl_find_named_fmt(e->vk->gpu, "rgba8"),
                                                .sampleable = true, .initial_data = decoded.rgba));
        free(decoded.rgba);
        if (!c->path || !c->source) {
            evict(e, target);
            return fail(e, "Cannot upload decoded image");
        }
        e->stats.uploads++;
    }
    struct lw_options previous_options = e->options;
    int result = transition(e, target, NULL, options);
    if (result < 0)
        goto failed;
    if (result > 0) {
        fail(e, "Image transition cancelled");
        goto rollback_image;
    }
    e->first_presented = 0;
    e->first_discarded = 0;
    e->first_wait = 1;
    if (render(e, -1, target, 1, options.fit))
        goto failed;
    double deadline = image_now() + 3;
    while (!e->first_presented && !e->first_discarded && image_now() < deadline) {
        int r = events(e, 50);
        if (r < 0)
            goto failed;
        if (r > 0) {
            fail(e, "Image final presentation cancelled");
            goto rollback_image;
        }
    }
    if (!isfinite(e->first_presented) || !e->first_presented || e->first_discarded) {
        fail(e, "Image final frame was not presented");
        goto rollback_image;
    }
    e->first_wait = 0;
    stop_video(e);
    e->options = options;
    e->decode_reason[0] = 0;
    e->fallback_reason[0] = 0;
    e->current = target;
    e->active = 1;
    for (int i = 0; i < 2; i++)
        if (i != target)
            pl_tex_destroy(e->vk->gpu, &e->images[i].scaled);
    clear_transition(e);
    return 0;
rollback_image:
    e->first_wait = 0;
    if (e->video) {
        if (render_video(e, e->video))
            goto failed;
    } else if (e->active && e->current >= 0) {
        if (render(e, -1, e->current, 1, previous_options.fit))
            goto failed;
    } else
        destroy_surface(e);
    clear_transition(e);
    return -1;
failed:
    e->first_wait = 0;
    // A failed acquired/submitted GPU frame cannot be safely replayed in place.
    // Decode/config validation errors return earlier and retain the old surface.
    e->options = previous_options;
    e->fatal = 1;
    destroy_surface(e);
    return -2;
}

#include "engine_video.h"

int lw_pump(struct lw_engine *e, int timeout) {
    if (e->video)
        return pump_video(e, timeout);
    int r = events(e, timeout);
    if (r < 0) {
        e->fatal = 1;
        destroy_surface(e);
        return fail(e, "Wayland output or connection lost");
    }
    if (!r && e->dirty && e->active && e->current >= 0) {
        if (render(e, -1, e->current, 1, e->options.fit)) {
            e->fatal = 1;
            destroy_surface(e);
            return -1;
        }
    }
    return r;
}
void lw_hide(struct lw_engine *e) {
    if (!e)
        return;
    stop_video(e);
    e->decode_reason[0] = 0;
    e->fallback_reason[0] = 0;
    destroy_surface(e);
    for (int i = 0; i < 2; i++)
        evict(e, i);
    clear_transition(e);
    e->current = -1;
    if (e->display)
        wl_display_flush(e->display);
}
void lw_destroy(struct lw_engine *e) {
    if (!e)
        return;
    stop_video(e);
    av_buffer_unref(&e->hardware);
    e->decode_reason[0] = 0;
    e->fallback_reason[0] = 0;
    destroy_surface(e);
    for (int i = 0; i < 2; i++)
        evict(e, i);
    if (e->vk) {
        clear_transition(e);
        pl_renderer_destroy(&e->renderer);
        pl_dispatch_destroy(&e->dispatch);
        pl_gpu_finish(e->vk->gpu);
        pl_vulkan_destroy(&e->vk);
    }
    pl_vk_inst_destroy(&e->instance);
    pl_log_destroy(&e->log);
    for (int i = 0; i < MAX_OUTPUTS; i++)
        if (e->outputs[i].object)
            wl_output_release(e->outputs[i].object);
    if (e->presentation)
        wp_presentation_destroy(e->presentation);
    if (e->fractional_manager)
        wp_fractional_scale_manager_v1_destroy(e->fractional_manager);
    if (e->viewporter)
        wp_viewporter_destroy(e->viewporter);
    if (e->shell)
        zwlr_layer_shell_v1_destroy(e->shell);
    if (e->compositor)
        wl_compositor_destroy(e->compositor);
    if (e->registry)
        wl_registry_destroy(e->registry);
    if (e->display)
        wl_display_disconnect(e->display);
    free(e);
}

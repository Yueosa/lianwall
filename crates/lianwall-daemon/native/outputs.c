#include "outputs.h"
#include <errno.h>
#include <poll.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <wayland-client.h>
struct probe_entry {
    struct wl_output *object;
    struct lw_output_info info;
};
struct probe {
    struct wl_display *display;
    struct wl_registry *registry;
    struct probe_entry entries[LW_OUTPUT_LIMIT];
    struct lw_output_snapshot *result;
};
static double now(void) {
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return t.tv_sec + t.tv_nsec / 1e9;
}
static void release(struct probe_entry *entry) {
    if (!entry->object)
        return;
    if (entry->info.version >= 3)
        wl_output_release(entry->object);
    else
        wl_output_destroy(entry->object);
    memset(entry, 0, sizeof(*entry));
}
static void geometry(void *data, struct wl_output *output, int32_t x, int32_t y, int32_t pw,
                     int32_t ph, int32_t subpixel, const char *make, const char *model,
                     int32_t transform) {
    (void)output;
    (void)x;
    (void)y;
    (void)pw;
    (void)ph;
    (void)subpixel;
    (void)make;
    (void)model;
    ((struct probe_entry *)data)->info.transform = transform;
}
static void mode(void *data, struct wl_output *output, uint32_t flags, int32_t w, int32_t h,
                 int32_t refresh) {
    (void)output;
    if (flags & WL_OUTPUT_MODE_CURRENT) {
        struct lw_output_info *info = &((struct probe_entry *)data)->info;
        info->width = w;
        info->height = h;
        info->refresh_millihz = refresh;
    }
}
static void done(void *data, struct wl_output *output) {
    (void)data;
    (void)output;
}
static void scale(void *data, struct wl_output *output, int32_t value) {
    (void)output;
    ((struct probe_entry *)data)->info.scale = value > 0 ? value : 1;
}
static void name(void *data, struct wl_output *output, const char *value) {
    (void)output;
    struct lw_output_info *info = &((struct probe_entry *)data)->info;
    if (strlen(value) >= sizeof(info->name))
        info->name[0] = 0;
    else
        snprintf(info->name, sizeof(info->name), "%s", value);
}
static void description(void *data, struct wl_output *output, const char *value) {
    (void)output;
    snprintf(((struct probe_entry *)data)->info.description, 256, "%s", value);
}
static const struct wl_output_listener listener = {geometry, mode, done, scale, name, description};
static void global(void *data, struct wl_registry *registry, uint32_t id, const char *interface,
                   uint32_t version) {
    struct probe *probe = data;
    struct lw_output_snapshot *result = probe->result;
    if (!strcmp(interface, "wl_compositor"))
        result->compositor = 1;
    else if (!strcmp(interface, "zwlr_layer_shell_v1"))
        result->layer_shell = 1;
    else if (!strcmp(interface, "wp_viewporter"))
        result->viewporter = 1;
    else if (!strcmp(interface, "wp_fractional_scale_manager_v1"))
        result->fractional_scale = 1;
    else if (!strcmp(interface, "wp_presentation"))
        result->presentation = 1;
    else if (!strcmp(interface, "wl_output")) {
        for (int i = 0; i < LW_OUTPUT_LIMIT; i++) {
            struct probe_entry *entry = &probe->entries[i];
            if (entry->object)
                continue;
            entry->info.registry_id = id;
            entry->info.version = version < 4 ? version : 4;
            entry->info.scale = 1;
            entry->object =
                wl_registry_bind(registry, id, &wl_output_interface, entry->info.version);
            wl_output_add_listener(entry->object, &listener, entry);
            return;
        }
        result->truncated = 1;
    }
}
static void removed(void *data, struct wl_registry *registry, uint32_t id) {
    (void)registry;
    struct probe *probe = data;
    for (int i = 0; i < LW_OUTPUT_LIMIT; i++)
        if (probe->entries[i].object && probe->entries[i].info.registry_id == id)
            release(&probe->entries[i]);
}
static const struct wl_registry_listener registry_listener = {global, removed};
static void synced(void *data, struct wl_callback *callback, uint32_t serial) {
    (void)callback;
    (void)serial;
    *(int *)data = 1;
}
static const struct wl_callback_listener sync_listener = {synced};
static int roundtrip(struct wl_display *display, double deadline) {
    int complete = 0, result = -1;
    struct wl_callback *callback = wl_display_sync(display);
    if (!callback)
        return -1;
    wl_callback_add_listener(callback, &sync_listener, &complete);
    while (!complete && now() < deadline) {
        while (wl_display_prepare_read(display) != 0)
            if (wl_display_dispatch_pending(display) < 0)
                goto finished;
        if (complete) {
            wl_display_cancel_read(display);
            result = 0;
            goto finished;
        }
        int flushed = wl_display_flush(display);
        if (flushed < 0 && errno != EAGAIN) {
            wl_display_cancel_read(display);
            goto finished;
        }
        struct pollfd fd = {.fd = wl_display_get_fd(display),
                            .events = POLLIN | (flushed < 0 ? POLLOUT : 0)};
        int timeout = (int)((deadline - now()) * 1000);
        if (timeout < 0)
            timeout = 0;
        int polled = poll(&fd, 1, timeout);
        if (polled > 0 && (fd.revents & POLLIN)) {
            if (wl_display_read_events(display) < 0)
                goto finished;
        } else
            wl_display_cancel_read(display);
        if ((polled < 0 && errno != EINTR) || (fd.revents & (POLLHUP | POLLERR | POLLNVAL)))
            goto finished;
        if (wl_display_dispatch_pending(display) < 0)
            goto finished;
    }
    if (complete)
        result = 0;
finished:
    wl_callback_destroy(callback);
    return result;
}
int lw_query_outputs(struct lw_output_snapshot *result, int timeout_ms) {
    memset(result, 0, sizeof(*result));
    /* WAYLAND_SOCKET can designate an inherited, single-use connection. Never
     * consume it (or change process environment) from this discovery thread. */
    if (getenv("WAYLAND_SOCKET")) {
        snprintf(
            result->error, sizeof(result->error),
            "Output discovery requires a separate WAYLAND_DISPLAY connection, not WAYLAND_SOCKET");
        return -1;
    }
    const char *display_name = getenv("WAYLAND_DISPLAY");
    if ((!display_name || display_name[0] != '/') && !getenv("XDG_RUNTIME_DIR")) {
        snprintf(result->error, sizeof(result->error),
                 "XDG_RUNTIME_DIR is unavailable for the daemon Wayland session");
        return -1;
    }
    struct probe probe = {.result = result};
    probe.display = wl_display_connect(NULL);
    if (!probe.display) {
        snprintf(result->error, sizeof(result->error),
                 "Cannot connect to the daemon's Wayland session");
        return -1;
    }
    probe.registry = wl_display_get_registry(probe.display);
    wl_registry_add_listener(probe.registry, &registry_listener, &probe);
    double deadline = now() + timeout_ms / 1000.0;
    int status = roundtrip(probe.display, deadline);
    if (!status)
        status = roundtrip(probe.display, deadline);
    for (int i = 0; i < LW_OUTPUT_LIMIT; i++) {
        if (!status && probe.entries[i].object)
            result->outputs[result->count++] = probe.entries[i].info;
        release(&probe.entries[i]);
    }
    wl_registry_destroy(probe.registry);
    wl_display_flush(probe.display);
    wl_display_disconnect(probe.display);
    if (status)
        snprintf(result->error, sizeof(result->error),
                 "Wayland output discovery timed out or disconnected");
    return status;
}

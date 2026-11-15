#ifndef LIANWALL_OUTPUTS_H
#define LIANWALL_OUTPUTS_H
#include <stdint.h>
#define LW_OUTPUT_LIMIT 32
struct lw_output_info {
    uint32_t registry_id, version;
    int32_t width, height, refresh_millihz, scale, transform;
    char name[128], description[256];
};
struct lw_output_snapshot {
    int32_t count, truncated, compositor, layer_shell, viewporter, fractional_scale, presentation;
    struct lw_output_info outputs[LW_OUTPUT_LIMIT];
    char error[256];
};
/* Independent registry connection; no GPU or existing owner access. */
int lw_query_outputs(struct lw_output_snapshot *, int timeout_ms);
#endif

#ifndef LIANWALL_TRANSITION_H
#define LIANWALL_TRANSITION_H
#include <libplacebo/dispatch.h>
#include <libplacebo/shaders/custom.h>
/* Blend pre-scaled SDR targets on the GPU; only the uniform changes per frame. */
static bool crossfade(pl_dispatch dispatch, pl_tex previous, pl_tex next, pl_tex target,
                      float progress) {
    struct pl_shader_desc textures[2] = {
        {.desc = {.name = "previous_image", .type = PL_DESC_SAMPLED_TEX},
         .binding = {.object = previous}},
        {.desc = {.name = "next_image", .type = PL_DESC_SAMPLED_TEX}, .binding = {.object = next}},
    };
    struct pl_shader_var variable = {
        .var = pl_var_float("progress"), .data = &progress, .dynamic = true};
    pl_dispatch_reset_frame(dispatch);
    pl_shader shader = pl_dispatch_begin(dispatch);
    struct pl_custom_shader custom = {
        .description = "LianWall crossfade",
        .body = "ivec2 p = ivec2(gl_FragCoord.xy); color = mix(texelFetch(previous_image, p, 0), "
                "texelFetch(next_image, p, 0), progress);",
        .output = PL_SHADER_SIG_COLOR,
        .descriptors = textures,
        .num_descriptors = 2,
        .variables = &variable,
        .num_variables = 1,
    };
    if (!pl_shader_custom(shader, &custom)) {
        pl_dispatch_abort(dispatch, &shader);
        return false;
    }
    return pl_dispatch_finish(dispatch, pl_dispatch_params(.shader = &shader, .target = target));
}
#endif

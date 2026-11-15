//! A short, independent Wayland registry query, isolated from the renderer owner.
use lianwall_core::socket::{OutputInfo, OutputProtocols, OutputSnapshot};
use std::ffi::{CStr, c_char};
#[repr(C)]
#[derive(Clone, Copy)]
struct RawOutput {
    registry_id: u32,
    version: u32,
    width: i32,
    height: i32,
    refresh_millihz: i32,
    scale: i32,
    transform: i32,
    name: [c_char; 128],
    description: [c_char; 256],
}
#[repr(C)]
struct RawSnapshot {
    count: i32,
    truncated: i32,
    compositor: i32,
    layer_shell: i32,
    viewporter: i32,
    fractional_scale: i32,
    presentation: i32,
    outputs: [RawOutput; 32],
    error: [c_char; 256],
}
unsafe extern "C" {
    fn lw_query_outputs(snapshot: *mut RawSnapshot, timeout_ms: i32) -> i32;
}
fn text(bytes: &[c_char]) -> Option<String> {
    // All fields are zero-initialized and populated with bounded C strings.
    let value = unsafe { CStr::from_ptr(bytes.as_ptr()) }.to_string_lossy();
    (!value.is_empty()).then(|| value.into_owned())
}
pub fn discover() -> OutputSnapshot {
    // SAFETY: this C POD contains only integers and byte arrays. The C function
    // owns its separate display connection and never accesses renderer objects.
    let mut raw: RawSnapshot = unsafe { std::mem::zeroed() };
    let status = unsafe { lw_query_outputs(&mut raw, 1500) };
    let protocols = OutputProtocols {
        compositor: raw.compositor != 0,
        layer_shell: raw.layer_shell != 0,
        viewporter: raw.viewporter != 0,
        fractional_scale: raw.fractional_scale != 0,
        presentation_time: raw.presentation != 0,
    };
    let compatible = protocols.compositor
        && protocols.layer_shell
        && protocols.viewporter
        && protocols.fractional_scale
        && protocols.presentation_time;
    let outputs = raw.outputs[..raw.count.clamp(0, 32) as usize]
        .iter()
        .map(|entry| {
            let name = text(&entry.name);
            OutputInfo {
                registry_id: entry.registry_id,
                native_selectable: compatible
                    && entry.version >= 4
                    && name
                        .as_ref()
                        .is_some_and(|name| name != "*" && !name.contains(',')),
                name,
                description: text(&entry.description),
                width: (entry.width > 0).then_some(entry.width),
                height: (entry.height > 0).then_some(entry.height),
                refresh_millihz: (entry.refresh_millihz > 0).then_some(entry.refresh_millihz),
                integer_scale: entry.scale,
                transform: entry.transform,
            }
        })
        .collect();
    OutputSnapshot {
        schema_version: 1,
        available: status == 0,
        truncated: raw.truncated != 0,
        protocols,
        outputs,
        error: text(&raw.error),
    }
}

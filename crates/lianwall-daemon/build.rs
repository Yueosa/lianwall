#[cfg(feature = "native-image")]
fn main() {
    use std::{env, path::PathBuf, process::Command};
    fn output(program: &str, args: &[&str]) -> String {
        let result = Command::new(program).args(args).output().expect(program);
        assert!(
            result.status.success(),
            "{program}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        String::from_utf8(result.stdout).unwrap().trim().to_owned()
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let protocols = output(
        "pkg-config",
        &["--variable=pkgdatadir", "wayland-protocols"],
    );
    let mut build = cc::Build::new();
    build
        .include(&out)
        .include("native")
        .file("native/engine.c")
        .file("native/outputs.c")
        .file("native/video.c")
        .file("native/video_gpu.c")
        .file("native/video_queue.c")
        .file("native/video_hw.c")
        .flag("-pthread")
        .flag("-std=c11")
        .flag("-D_POSIX_C_SOURCE=200809L")
        .opt_level(2);
    for (name, xml) in [
        ("layer", "native/wlr-layer-shell.xml".to_string()),
        ("xdg", format!("{protocols}/stable/xdg-shell/xdg-shell.xml")),
        (
            "viewporter",
            format!("{protocols}/stable/viewporter/viewporter.xml"),
        ),
        (
            "fractional",
            format!("{protocols}/staging/fractional-scale/fractional-scale-v1.xml"),
        ),
        (
            "presentation",
            format!("{protocols}/stable/presentation-time/presentation-time.xml"),
        ),
    ] {
        let header = out.join(format!("{name}-client.h"));
        let code = out.join(format!("{name}-protocol.c"));
        output(
            "wayland-scanner",
            &["client-header", &xml, header.to_str().unwrap()],
        );
        output(
            "wayland-scanner",
            &["private-code", &xml, code.to_str().unwrap()],
        );
        build.file(code);
        println!("cargo:rerun-if-changed={xml}");
    }
    for flag in output(
        "pkg-config",
        &[
            "--cflags",
            "libplacebo",
            "vulkan",
            "wayland-client",
            "libavformat",
            "libavcodec",
            "libavutil",
            "libswscale",
            "lcms2",
        ],
    )
    .split_whitespace()
    {
        build.flag(flag);
    }
    if let Some(headers) = env::var_os("VULKAN_HEADERS") {
        build.include(headers);
    }
    println!("cargo:rerun-if-env-changed=VULKAN_HEADERS");
    println!("cargo:rerun-if-changed=native");
    build.compile("lianwall_native_image");
    for flag in output(
        "pkg-config",
        &[
            "--libs",
            "libplacebo",
            "vulkan",
            "wayland-client",
            "libavformat",
            "libavcodec",
            "libavutil",
            "libswscale",
            "lcms2",
        ],
    )
    .split_whitespace()
    {
        if let Some(lib) = flag.strip_prefix("-l") {
            println!("cargo:rustc-link-lib={lib}");
        } else if let Some(path) = flag.strip_prefix("-L") {
            println!("cargo:rustc-link-search=native={path}");
        } else {
            println!("cargo:rustc-link-arg={flag}");
        }
    }
    println!("cargo:rustc-link-lib=m");
    println!("cargo:rustc-link-lib=pthread");
}
#[cfg(not(feature = "native-image"))]
fn main() {}

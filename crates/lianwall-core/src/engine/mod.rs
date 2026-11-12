//! Executable discovery shared by tools. Engine lifecycle belongs to lianwall-daemon::backend.
mod detect;
mod r#struct;
pub use detect::{detect, detect_image_bin, executable_in_path};
pub use r#struct::DetectOutput;

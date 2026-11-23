//! Says which GPU rustudio runs on and why it lost it. A lost device (driver
//! reset, a Windows TDR on a long frame, out of GPU memory) used to surface
//! as an egui-wgpu staging-buffer panic with no cause; now the reason goes to
//! the diagnostic log and the crash report.

use egui_wgpu::wgpu;
use std::sync::OnceLock;

static ADAPTER: OnceLock<String> = OnceLock::new();
static LOST: OnceLock<String> = OnceLock::new();

fn describe(info: &wgpu::AdapterInfo) -> String {
    format!(
        "{} ({:?}, {:?}, driver {} {})",
        info.name, info.backend, info.device_type, info.driver, info.driver_info
    )
}

/// Log the adapter and watch the device for loss. Call once per device.
pub fn watch(adapter: &wgpu::Adapter, device: &wgpu::Device) {
    let gpu = describe(&adapter.get_info());
    log::info!(target: "rustudio::gpu", "GPU: {gpu}");
    let _ = ADAPTER.set(gpu);
    device.set_device_lost_callback(|reason, message| {
        if matches!(reason, wgpu::DeviceLostReason::Destroyed) {
            return;
        }
        let gpu = ADAPTER.get().map(String::as_str).unwrap_or("unknown GPU");
        let text = format!("GPU device lost on {gpu}: {message}");
        log::error!(target: "rustudio::gpu", "{text}");
        let _ = LOST.set(text);
    });
}

/// Why the device was lost, once it has been.
pub fn lost() -> Option<&'static str> {
    LOST.get().map(String::as_str)
}

/// Call at the top of a frame: once the device is lost nothing more can be
/// drawn, so stop with the reason in the crash report (shown on the next
/// start) instead of a frozen window.
pub fn check() {
    if let Some(why) = lost() {
        panic!("{why}. Update the graphics driver; WGPU_BACKEND=dx12 or WGPU_BACKEND=vulkan picks the other graphics API.");
    }
}

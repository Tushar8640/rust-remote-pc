use anyhow::{anyhow, Result};
use image::RgbaImage;
use tracing::info;
use xcap::Monitor;

pub struct ScreenCapturer {
    monitor: Monitor,
}

unsafe impl Send for ScreenCapturer {}
unsafe impl Sync for ScreenCapturer {}

impl ScreenCapturer {
    pub fn new() -> Result<Self> {
        let monitors = Monitor::all().map_err(|e| anyhow!("Failed to enumerate monitors: {:?}", e))?;
        if monitors.is_empty() {
            return Err(anyhow!("No monitors detected"));
        }

        // Find primary monitor, or fallback to the first monitor
        let primary = monitors
            .into_iter()
            .find(|m| m.is_primary())
            .unwrap_or_else(|| {
                info!("No monitor flagged as primary, using first detected monitor");
                Monitor::all().unwrap().remove(0)
            });

        info!(
            "ScreenCapturer initialized on monitor: {} ({}x{})",
            primary.name(),
            primary.width(),
            primary.height()
        );

        Ok(Self { monitor: primary })
    }

    pub fn dimensions(&self) -> (u32, u32) {
        (self.monitor.width(), self.monitor.height())
    }

    /// Captures the current frame and returns an RgbaImage
    pub fn capture(&self) -> Result<RgbaImage> {
        let image = self
            .monitor
            .capture_image()
            .map_err(|e| anyhow!("Screen capture failed: {:?}", e))?;
        Ok(image)
    }
}

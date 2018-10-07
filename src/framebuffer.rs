use std::os::raw::c_void;

use drm::control::framebuffer as dev_fb;
use gbm;
pub use gbm::Format;
use egl;

use device::Gpu;

pub type Image = *const c_void;

pub struct Framebuffer {
    pub drm_fb: dev_fb::Handle,
    pub width: u32,
    pub height: u32,
}

impl Framebuffer {
    pub fn handle(&self) -> dev_fb::Handle {
        self.drm_fb
    }
}
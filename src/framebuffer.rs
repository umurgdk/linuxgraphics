use drm::control::framebuffer as dev_fb;

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
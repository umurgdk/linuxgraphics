use std::ptr;
use std::ops::Deref;
use std::os::unix::io::AsRawFd;

use drm::control::crtc;
use drm::control::encoder;
use drm::control::connector;
use drm::control::Mode as DrmMode;
use drm::control::framebuffer as drm_fb;
use drm::control::ResourceInfo;

use egl;
use gbm;
use gbm::BufferObjectFlags;

use device::{Gpu, DeviceFile};
use framebuffer::{Framebuffer, Format};

pub struct Display {
    pub identifier: String,
    pub modes: Vec<DrmMode>,
    pub connector: connector::Handle,
    pub encoder: Option<encoder::Info>
}

impl Display {
    pub fn current_crtc(&self, gpu: &Gpu) -> Option<crtc::Info> {
        self.encoder
            .and_then(|enc| enc.current_crtc())
            .and_then(|crtc| gpu.get_crtc(crtc))
    }
}

pub struct Surface {
    egl_display: egl::EGLDisplay,
    egl_context: egl::EGLContext,
    egl_surface: egl::EGLSurface,
    gbm_surface: gbm::Surface<drm_fb::Handle>,
    pub mode: DrmMode,
    pub format: Format,
    pub framebuffer: Option<Framebuffer>,
    crtc: crtc::Handle,
    current_bo: Option<gbm::SurfaceBufferHandle<drm_fb::Handle>>,
    next_bo: Option<gbm::SurfaceBufferHandle<drm_fb::Handle>>,
}

impl Surface {
    pub fn new(egl_display: egl::EGLDisplay, egl_context: egl::EGLContext, egl_surface: egl::EGLSurface, gbm_surface: gbm::Surface<drm_fb::Handle>, crtc: crtc::Handle, mode: DrmMode, format: Format) -> Surface {
        Surface { egl_display, egl_context, egl_surface, gbm_surface, mode, format, framebuffer: None, current_bo: None, next_bo: None, crtc }
    }

    pub fn make_current(&self) {
        if !egl::make_current(self.egl_display, self.egl_surface, self.egl_surface, self.egl_context) {
            panic!("[egl] failed to make DisplaySurface the current one");
        }
    }

    pub fn swap_buffers(&mut self, gpu: &Gpu) {
        egl::swap_buffers(self.egl_display, self.egl_surface);

        let mut gbm_bo = self.gbm_surface.lock_front_buffer()
            .expect("[gbm] failed to lock front buffer");

        let drm_fb = Self::get_framebuffer_from_gbm_buffer(gpu, &mut gbm_bo);

        let (width, height) = self.mode.size();

        if self.current_bo.is_none() {
            self.current_bo = Some(gbm_bo);
        } else {
            self.next_bo = Some(gbm_bo);
        }

        self.framebuffer = Some(Framebuffer{ drm_fb, width: width as u32, height: height as u32 });
    }

    pub fn present(&mut self, gpu: &Gpu) {
        self.swap_buffers(gpu);
        gpu.page_flip(self.crtc, self);

        'waitloop:
        loop {
            let events = gpu.receive_events();
            for e in events {
                match e {
                    crtc::Event::PageFlip(_) => break 'waitloop,
                    _ => {}
                }
            }
        }

        self.current_bo.take();
        self.current_bo = self.next_bo.take();
    }

    fn get_framebuffer_from_gbm_buffer(gpu: &Gpu, bo: &mut gbm::SurfaceBufferHandle<drm_fb::Handle>) -> drm_fb::Handle {
        if let Ok(Some(handle)) = bo.userdata() {
            return handle.to_owned();
        }

        let handle = {
            let buf_obj: &gbm::BufferObject<drm_fb::Handle> = bo.deref();
            let fb = drm_fb::create(gpu.deref(), buf_obj)
                .expect("[drm] failed to create framebuffer");
            fb.handle()
        };

        bo.set_userdata(handle);
        handle
    }
}
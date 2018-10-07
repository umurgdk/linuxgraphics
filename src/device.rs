use std::fs::{File, OpenOptions};
use std::ops::Deref;
use std::os::unix::io::RawFd;
use std::os::unix::io::AsRawFd;
use std::os::unix::io::IntoRawFd;
use std::ptr;

use drm::Device as DrmDevice;
use drm::control::Device as DrmControlDevice;
use drm::control::framebuffer as drm_fb;
use drm::control::{ResourceHandle, ResourceInfo};
use drm::control::{self, connector, encoder, crtc, Mode};

use gbm;
use gbm::AsRaw;

use egl;

use display::{Display, Surface as DisplaySurface};
use framebuffer::Framebuffer;

pub struct DeviceFile(File);

impl AsRawFd for DeviceFile {
    fn as_raw_fd(&self) -> RawFd {
        self.0.as_raw_fd()
    }
}

impl IntoRawFd for DeviceFile {
    fn into_raw_fd(self) -> RawFd {
        self.0.into_raw_fd()
    }
}

impl DrmDevice for DeviceFile {}
impl DrmControlDevice for DeviceFile {}

pub struct Gpu {
    pub gbm_device: gbm::Device<DeviceFile>,
    pub connectors: Vec<connector::Info>,
    pub encoders: Vec<encoder::Info>,
    pub crtcs: Vec<crtc::Info>,
}

impl Deref for Gpu {
    type Target = gbm::Device<DeviceFile>;

    fn deref(&self) -> &Self::Target {
        &self.gbm_device
    }
}

//impl AsRawFd for Gpu {
//    fn as_raw_fd(&self) -> RawFd {
//        self.gbm_device.as_raw_fd()
//    }
//}
//
//impl IntoRawFd for Gpu {
//    fn into_raw_fd(self) -> RawFd {
//        self.gbm_device.into_raw_fd()
//    }
//}
//
//impl DrmDevice for Gpu {}
//impl DrmControlDevice for Gpu {}

impl Gpu {
    pub fn get_crtc(&self, crtc: crtc::Handle) -> Option<crtc::Info> {
        self.crtcs.iter().find(|c| c.handle() == crtc).map(|c| c.to_owned())
    }

    pub fn displays(&self) -> Vec<Display> {
        self.connectors
            .iter()
            .cloned()
            .filter(|c| c.connection_state() == connector::State::Connected)
            .enumerate()
            .map(|(i, c)| {
                let modes = c.modes().to_owned();
                let connector = c.handle();
                let identifier = format!("Display {}", i);
                let encoder = c.current_encoder().and_then(|cur_enc| {
                    self.encoders.iter().find(|enc| enc.handle() == cur_enc)
                }).map(|enc| enc.to_owned());
                Display { identifier, connector, modes, encoder }
            })
            .collect()
    }

    pub fn modeset(&self, crt: crtc::Info, displays: &[&Display], surface: &DisplaySurface) {
        let connections = displays.into_iter().map(|d| d.connector).collect::<Vec<_>>();

        let framebuffer = surface.framebuffer.as_ref().expect("[gpu] display surface doesn't have a framebuffer");
        crtc::set(&self.gbm_device, crt.handle(), framebuffer.handle(), &connections, (0, 0), Some(surface.mode))
            .expect("[drm] failed to set mode");
    }

    pub fn modeset_by_crtc(&self, conn: connector::Handle, crt: crtc::Info) {
        crtc::set(&self.gbm_device, crt.handle(), crt.fb(), &[conn], crt.position(), crt.mode())
            .expect("[drm] failed to set mode by crtc");
    }

    pub fn page_flip(&self, crtc: crtc::Handle, surface: &DisplaySurface) {
        let fb = surface.framebuffer.as_ref().expect("[gpu] cannot do pageflip. display surface has no framebuffer");

        crtc::page_flip(&self.gbm_device, crtc, fb.handle(), &[crtc::PageFlipFlags::PageFlipEvent])
            .expect("[gpu] page flip failed");
    }

    pub fn receive_events(&self) -> crtc::Events {
        crtc::receive_events(&self.gbm_device)
            .expect("[gpu] failed receive crtc events")
    }

    pub fn initialize_display(&self, display: &Display, crtc: crtc::Handle, format: gbm::Format, mode: Mode) -> DisplaySurface {
        use cognitive_graphics::egl_tools;

        let egl_display = egl_tools::get_gbm_display(self.gbm_device.as_raw() as _)
            .expect("Failed to get gbm display");

        let mut maj: egl::EGLint = 0;
        let mut min: egl::EGLint = 0;
        if !egl::initialize(egl_display, &mut maj, &mut min) {
            panic!("[egl] failed to initialize EGL");
        }

        println!("EGL major: {}, minor: {}", maj, min);

        if !egl::bind_api(egl::EGL_OPENGL_API) {
            panic!("[egl] failed to bing OpenGL api")
        }

        const CONFIG_ATTRIBS: [egl::EGLint; 13] = [
            egl::EGL_SURFACE_TYPE, egl::EGL_WINDOW_BIT,
            egl::EGL_RED_SIZE, 1,
            egl::EGL_GREEN_SIZE, 1,
            egl::EGL_BLUE_SIZE, 1,
            egl::EGL_ALPHA_SIZE, 0,
            egl::EGL_RENDERABLE_TYPE, egl::EGL_OPENGL_BIT,
            egl::EGL_NONE
        ];

        let config = egl::choose_config(egl_display, &CONFIG_ATTRIBS, 1)
            .expect("[egl] failed to choose configuration");

        const CONTEXT_ATTRIB_LIST: [egl::EGLint; 3] = [
            egl::EGL_CONTEXT_CLIENT_VERSION, 2,
            egl::EGL_NONE
        ];

        let egl_context = egl::create_context(egl_display, ptr::null_mut(), egl::EGL_NO_CONTEXT, &CONTEXT_ATTRIB_LIST)
            .expect("[egl] failed to create context");

        let (width, height) = mode.size();
        let surface = self.gbm_device.create_surface(width as u32, height as u32, format, gbm::BufferObjectFlags::SCANOUT | gbm::BufferObjectFlags::RENDERING)
            .expect("[gbm] failed to create surface");

        let egl_surface = egl::create_window_surface(egl_display, config, surface.as_raw() as _, &[])
            .expect("[egl] failed to create window surface");

        DisplaySurface::new(egl_display, egl_context, egl_surface, surface, crtc, mode, format)
    }
}

pub fn open(path: &'static str) -> Gpu {
    let mut options = OpenOptions::new();
    options.read(true);
    options.write(true);

    let gpu_file = options.open(path).expect(&format!("Failed to open {}", path));
    let gbm_device = gbm::Device::new(DeviceFile(gpu_file)).expect("Failed to create a gbm device");

    let resource_handles = gbm_device.resource_handles().expect("Failed to get resource handles from gbm device");
    let connectors = load_information(&gbm_device, resource_handles.connectors());
    let encoders = load_information(&gbm_device, resource_handles.encoders());
    let crtcs = load_information(&gbm_device, resource_handles.crtcs());

    Gpu { gbm_device, connectors, encoders, crtcs }
}

fn load_information<T, U>(card: &gbm::Device<DeviceFile>, handles: &[T]) -> Vec<U>
    where
        T: ResourceHandle,
        U: ResourceInfo<Handle = T>,
{
    handles
        .iter()
        .map(|&h| {
            card.resource_info(h).expect("Could not load resource info")
        })
        .collect()
}
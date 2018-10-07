extern crate drm;
extern crate rand;
extern crate gbm;
extern crate egl;
extern crate gl;
extern crate cognitive_graphics;
#[macro_use]
extern crate failure;

use std::ops::Deref;

use failure::Error;
use failure::ResultExt;

use drm::control::crtc::{Events, Event};
use drm::Device as DrmDevice;
use drm::control::Device as DrmControlDevice;
use drm::control::{ResourceHandle, ResourceInfo};

mod device;
mod display;
mod framebuffer;

fn main() {
    let gpu = device::open("/dev/dri/card0");
    let displays = gpu.displays();

    let display = displays.first().expect("No displays are attached");
    let crtc = gpu.crtcs.first().expect("No crtc is available").to_owned();
    let previous_crtc = display.current_crtc(&gpu);

    let native_mode = display.modes.first().expect("display doesn't support any modes").to_owned();
    let (display_w, display_h) = native_mode.size();

    let mut surface = gpu.initialize_display(display, crtc.handle(), gbm::Format::XRGB8888, native_mode);
    surface.make_current();

    surface.swap_buffers(&gpu);
    gpu.modeset(crtc, &[display], &surface);

    gl::load_with(|s| egl::get_proc_address(s) as *const std::os::raw::c_void);

    unsafe {
        use std::ffi::CStr;
        let version_ptr: *const u8 = gl::GetString(gl::VERSION);
        if !version_ptr.is_null() {
            let version = CStr::from_ptr(version_ptr as _);
            println!("OpenGL version: {}", version.to_str().unwrap_or("n/a"));
        }

        let vendor_ptr: *const u8 = gl::GetString(gl::VENDOR);
        if !vendor_ptr.is_null() {
            let vendor = CStr::from_ptr(vendor_ptr as _);
            println!("OpenGL vendor: {}", vendor.to_str().unwrap_or("n/a"));
        }

        let renderer_ptr: *const u8 = gl::GetString(gl::RENDERER);
        if !renderer_ptr.is_null() {
            let renderer = CStr::from_ptr(renderer_ptr as _);
            println!("OpenGL renderer: {}", renderer.to_str().unwrap_or("n/a"));
        }
    }

    let mut i = 0;
    while i < 100 {
        unsafe {
            gl::ClearColor(1.0 - (i as f32 / 100.0), 1.0, 1.0, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);
        }

        surface.present(&gpu);
        i += 1;
    }


    // TODO: handle surface and framebuffer destroy
    //    drm::control::framebuffer::destroy(gpu.deref(), front_fb.handle()).unwrap();

    if let Some(crtc_info) = previous_crtc {
        gpu.modeset_by_crtc(display.connector, crtc_info);
    }
}

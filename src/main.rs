extern crate drm;
extern crate rand;
extern crate gbm;
extern crate egl;
extern crate gl;
extern crate cognitive_graphics;
#[macro_use]
extern crate failure;
extern crate udev;
extern crate input;
extern crate dbus;
extern crate libc;

use drm::control::ResourceInfo;

use input::Event;
use input::event::KeyboardEvent;

mod device;
mod display;
mod framebuffer;
//mod input_interface;
//mod input_manager;

//use self::input_interface::InputInterface;

fn main() {
    let gpu = device::open("/dev/dri/card0");
    let displays = gpu.displays();

    let display = displays.first().expect("No displays are attached");
    let crtc = gpu.crtcs.first().expect("No crtc is available").to_owned();
    let previous_crtc = display.current_crtc(&gpu);

    let native_mode = display.modes.first().expect("display doesn't support any modes").to_owned();
    let (_display_w, _display_h) = native_mode.size();

    let mut surface = gpu.initialize_display(display, crtc.handle(), gbm::Format::XRGB8888, native_mode);
    surface.make_current();

    surface.swap_buffers(&gpu);
    gpu.modeset(crtc, &[display], &surface);

    // start input system
//    let udev_ctx = udev::Context::new().expect("[udev] failed to create context");
//    let input_files = InputInterface::new();
//    let mut input_ctx = input::Libinput::new_from_udev(input_files, &udev_ctx);
//    input_ctx.udev_assign_seat("seat0")
//        .expect("[udev] failed to assign seat0");

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

    let mut i = 0i64;
'mainloop:
    loop {
//        for input_e in input_ctx.clone() {
//            println!("[libinput] got event: {:?}", input_e);
//            match input_e {
//                Event::Keyboard(_) => {
//                    break 'mainloop;
//                },
//                _ => {}
//            }
//        }

        unsafe {
            gl::ClearColor(1.0 - ((i % 255) as f32 / 255.0), 1.0, 1.0, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);
        }

//        if let Err(err) = input_ctx.dispatch() {
//            println!("[libinput] failed to dispatch: {}", err);
//        }
        surface.present(&gpu);
        i += 1;
    }


    // TODO: handle surface and framebuffer destroy
    //    drm::control::framebuffer::destroy(gpu.deref(), front_fb.handle()).unwrap();

    if let Some(crtc_info) = previous_crtc {
        gpu.modeset_by_crtc(display.connector, crtc_info);
    }
}

extern crate drm;
extern crate rand;
extern crate gbm;
extern crate egl;
extern crate gl;
extern crate cognitive_graphics;

use std::fs::File;
use std::fs::OpenOptions;

use std::os::unix::io::RawFd;
use std::os::unix::io::AsRawFd;

use drm::buffer::Buffer;
use drm::buffer::PixelFormat;

use drm::Device as BasicDevice;
use drm::control::Device as ControlDevice;

use drm::control::ResourceInfo;
use drm::control::ResourceHandle;
use drm::control::{connector, crtc, dumbbuffer, framebuffer};
use drm::control::crtc::{Events, Event};

use gbm::AsRaw;

use cognitive_graphics::egl_tools;

#[derive(Debug)]
pub struct Card(File);

impl AsRawFd for Card {
    fn as_raw_fd(&self) -> RawFd {
        self.0.as_raw_fd()
    }
}

impl<'a> AsRawFd for &'a Card {
    fn as_raw_fd(&self) -> RawFd {
        self.0.as_raw_fd()
    }
}

impl drm::Device for Card {}
impl drm::control::Device for Card {}

impl Card {
    pub fn open(path: &str) -> Self {
        let mut options = OpenOptions::new();
        options.read(true);
        options.write(true);
        Card(options.open(path).unwrap())
    }

    pub fn open_global() -> Self {
        Self::open("/dev/dri/card0")
    }

    pub fn open_control() -> Self {
        Self::open("/dev/dri/controlD64")
    }
}

struct Framebuffer {
    fbo: gl::types::GLuint,
    info: framebuffer::Info,
    bo: gbm::BufferObject<()>,
}

const BUF_WIDTH: usize = 1280;
const BUF_HEIGHT: usize = 800;

const SQUARE_SIZE: usize = 300;

const BG_COLOR: (u8, u8, u8, u8) = (255, 255, 255, 255);
const RECT_COLOR: (u8, u8, u8, u8) = (255, 255, 0, 0);
const NUM_COLORS: usize = 4;

fn render(pixels: &mut [u8], frame: u64) {
    for row in 0..SQUARE_SIZE {
        for col in 0..SQUARE_SIZE {
            let pindex = ((row * BUF_WIDTH) + (frame as usize) + col) * NUM_COLORS;
            if pindex > (BUF_WIDTH * BUF_HEIGHT) * NUM_COLORS {
                continue;
            }

            // clear damaged area (1 pixel to the left for all rows)
            if frame > 0 {
                let pindex = ((row * BUF_WIDTH) + (frame as usize) - 1) * NUM_COLORS;
                let (a, r, g, b) = BG_COLOR;
                pixels[pindex + 0] = a;
                pixels[pindex + 1] = r;
                pixels[pindex + 2] = g;
                pixels[pindex + 3] = b;

                let pindex = ((row * BUF_WIDTH) + (frame as usize) - 2) * NUM_COLORS;
                if frame > 1 {
                    let (a, r, g, b) = BG_COLOR;
                    pixels[pindex + 0] = a;
                    pixels[pindex + 1] = r;
                    pixels[pindex + 2] = g;
                    pixels[pindex + 3] = b;
                }
            }

            let (a, r, g, b) = RECT_COLOR;
            pixels[pindex + 0] = b;
            pixels[pindex + 1] = g;
            pixels[pindex + 2] = r;
            pixels[pindex + 3] = a;
        }
    }
}

struct GlScene {
    vbo: gl::types::GLint,
    program: gl::types::GLint,
}

fn render_gl(fbo: gl::types::GLint, scene: &GlScene) {
    unsafe {
        gl::BindFramebuffer(gl::FRAMEBUFFER, fbo);
        gl::ClearColor(0.5, 0.5, 0.5, 1.0);
        gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);
        gl::GenBuffers(1, &mut vbo_vertices);

        // Load vetices.
        gl::BindBuffer(gl::ARRAY_BUFFER, vbo_vertices);
        gl::EnableVertexAttribArray(self.loc_position as _);
        gl::VertexAttribPointer(
            self.loc_position as _,
            3,
            gl::FLOAT,
            gl::FALSE,
            3 * float_size as egl::EGLint,
            std::ptr::null(),
        );

        gl::BufferData(gl::ARRAY_BUFFER,
                       vertices_size as isize,
                       vertices.as_ptr() as *const _,
                       gl::DYNAMIC_DRAW);

        // Draw
        gl::DrawArrays(gl::TRIANGLES, 0, 3);
        gl::Finish();
    }
}

fn main() {
    let card = Card::open_global();

    // Load the information.
    let res = card.resource_handles().expect("Could not load normal resource ids.");
    let coninfo: Vec<connector::Info> = load_information(&card, res.connectors());
    let crtcinfo: Vec<crtc::Info> = load_information(&card, res.crtcs());

    // Filter each connector until we find one that's connected.
    let con = coninfo
        .iter()
        .filter(|&i| i.connection_state() == connector::State::Connected)
        .next()
        .expect("No connected connectors");

    // Get the first (usually best) mode
    let &mode = con.modes().iter().next().expect(
        "No modes found on connector",
    );

    // Find a crtc and FB
    let crtc = crtcinfo.iter().next().expect("No crtcs found");

    let mut gbm_dev = gbm::Device::new(card).expect("Failed to initialize gbm");
    let display = egl::get_display(gbm_dev.as_raw_mut() as *mut _).expect("Failed to get display");

    let mut major: egl::EGLint = 0;
    let mut minor: egl::EGLint = 0;
    egl::initialize(display, &mut major, &mut minor);
    egl::bind_api(egl::EGL_OPENGL_API);

    let version = egl::query_string(display, egl::EGL_VERSION);
    if let Some(version) = version {
        println!("EGL Version: {}", version.to_str().unwrap_or("N/A"));
    }

    let extensions = egl::query_string(display, egl::EGL_EXTENSIONS)
        .expect("Failed to query EGL_EXTENSIONS")
        .to_str()
        .expect("failed to convert EGL_EXTENSIONS query to str");

    if !extensions.contains("EGL_KHR_surfaceless_context") {
        eprintln!("FATAL: No support for EGL_KHR_surfaceless_context");
        std::process::exit(1);
    }

    if !extensions.contains("EGL_KHR_create_context") {
        eprintln!("FATAL: No EGL_KHR_create_context support");
        std::process::exit(1);
    }

    const EGL_CONTEXT_MAJOR_VERSION_KHR: egl::EGLint = 0x3098;
    const EGL_CONTEXT_MINOR_VERSION_KHR: egl::EGLint = 0x30FB;
    const EGL_CONTEXT_OPENGL_PROFILE_MASK_KHR: egl::EGLint = 0x30FD;
    const EGL_CONTEXT_OPENGL_CORE_PROFILE_BIT_KHR: egl::EGLint = 0x00000001;

    let create_ctx_attribs: &[egl::EGLint] = &[
        EGL_CONTEXT_MAJOR_VERSION_KHR, 1,
        EGL_CONTEXT_MINOR_VERSION_KHR, 4,
        egl::EGL_NONE,
    ];

    let ctx: egl::EGLContext = match egl::create_context(display, std::ptr::null_mut(), egl::EGL_NO_CONTEXT, create_ctx_attribs) {
        Some(ctx) => ctx,
        None => {
            eprintln!("Failed to create opengl context: {}", egl_error_msg());
            std::process::exit(1);
        }
    };

    egl::make_current(display, egl::EGL_NO_SURFACE, egl::EGL_NO_SURFACE, ctx);
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


    let front_fb = create_frame_buffer(&gbm_dev, display, ctx, con, crtc).expect("Failed to create front buffer (egl)");
    let back_fb = create_frame_buffer(&gbm_dev, display, ctx, con, crtc).expect("Failed to create back buffer (egl)");

    // Select the pixel format
    let fmt = PixelFormat::XRGB8888;

    // Create a DB
    let mut front_buffer = dumbbuffer::DumbBuffer::create_from_device(&gbm_dev, (1280, 800), fmt)
        .expect("Could not create front dumb buffer");

    let mut back_buffer = dumbbuffer::DumbBuffer::create_from_device(&gbm_dev, (1280, 800), fmt)
        .expect("Could not create back dumb buffer");

    // Create an FB:
    let front_fb = framebuffer::create(&gbm_dev, &front_buffer).expect("Could not create FB");
    let back_fb = framebuffer::create(&gbm_dev, &back_buffer).expect("Could not create back FB");

    // Set the crtc
    // On many setups, this requires root access.
    crtc::set(
        &gbm_dev,
        crtc.handle(),
        front_fb.handle(),
        &[con.handle()],
        (0, 0),
        Some(mode),
    ).expect("Could not set CRTC");

    crtc::page_flip(
        &gbm_dev,
        crtc.handle(),
        front_fb.handle(),
        &[crtc::PageFlipFlags::PageFlipEvent],
    ).expect("Failed to queue Page Flip");

    let mut events: Events;
    let mut waiting_for_flip = true;
    let mut frames = 0;

    {
        let mut front_pixels = front_buffer.map(&gbm_dev).expect("Could not map back dumbbuffer");
        front_pixels.as_mut().iter_mut().for_each(|p| *p = 255);
        render(&mut front_pixels.as_mut(), 0);

        let mut back_pixels = back_buffer.map(&gbm_dev).expect("Could not map back dumbbuffer");
        back_pixels.as_mut().iter_mut().for_each(|p| *p = 255);

        while waiting_for_flip {
            events = crtc::receive_events(&gbm_dev).unwrap();
            for event in events {
                match event {
                    Event::Vblank(s) => {
                        println!("vblank...")
                    },
                    Event::PageFlip(s) => {
                        println!("PageFlipEvent:{}", s.frame);
                        if frames % 2 == 0 {
                            render(back_pixels.as_mut(), frames + 1);
                            crtc::page_flip(&gbm_dev, crtc.handle(), back_fb.handle(), &[crtc::PageFlipFlags::PageFlipEvent])
                                .expect("failed to flip to front buffer");
                        } else {
                            render(front_pixels.as_mut(), frames + 1);
                            crtc::page_flip(&gbm_dev, crtc.handle(), front_fb.handle(), &[crtc::PageFlipFlags::PageFlipEvent])
                                .expect("failed to flip to back buffer");
                        }

                        frames += 1;

                    }
                    Event::Unknown(s) => println!("unkonw event:{:?}", s),
                }
            }
        }
    }

    let five_seconds = ::std::time::Duration::from_millis(5000);
    ::std::thread::sleep(five_seconds);

    framebuffer::destroy(&gbm_dev, front_fb.handle()).unwrap();
    framebuffer::destroy(&gbm_dev, back_fb.handle()).unwrap();
    front_buffer.destroy(&gbm_dev).unwrap();
    back_buffer.destroy(&gbm_dev).unwrap();
}

fn create_frame_buffer(dev: &gbm::Device<Card>, display: egl::EGLDisplay, ctx: egl::EGLContext, conn: &connector::Info, crtc: &crtc::Info) -> Result<Framebuffer, Box<dyn std::error::Error>> {
    let format = gbm::Format::ARGB8888;
    let buff_obj = dev.create_buffer_object::<()>(BUF_WIDTH as u32, BUF_HEIGHT as u32, format, gbm::BufferObjectFlags::SCANOUT | gbm::BufferObjectFlags::RENDERING)?;

    use cognitive_graphics::egl_tools::*;
    use cognitive_graphics::gl_tools::*;

    use std::ptr;
    let egl_create_image_khr = cognitive_graphics::egl_tools::get_proc_addr_of_create_image_khr().expect("No create_image_khr".into());
    let gl_egl_image_target_render_storage = get_proc_addr_of_image_target_render_storage_oes().ok_or("No image_texture_2d_oes")?;

    const EGL_NATIVE_PIXMAP_KHR: u32 = 0x30B0;
    let mut fbo: gl::types::GLuint = 0;


    unsafe {
        gl::GenFramebuffers(1, &mut fbo);
        gl::BindFramebuffer(gl::FRAMEBUFFER, fbo);

        let image = egl_create_image_khr(display, ptr::null_mut(), EGL_NATIVE_PIXMAP_KHR, buff_obj.as_raw_mut() as *mut _, ptr::null());
        if image.is_null() {
            return Err("failed to create framebuffer".into());
        }

        let mut color_rb: gl::types::GLuint = 0;
        gl::GenRenderbuffers(1, &mut color_rb);
        gl::BindRenderbuffer(gl::RENDERBUFFER, color_rb);
        gl_egl_image_target_render_storage(gl::RENDERBUFFER, image);
        gl::FramebufferRenderbuffer(gl::FRAMEBUFFER, gl::COLOR_ATTACHMENT0, gl::RENDERBUFFER, color_rb);

        let mut depth_rb: gl::types::GLuint = 0;
        gl::GenRenderbuffers(1, &mut depth_rb);
        gl::BindRenderbuffer(gl::RENDERBUFFER, depth_rb);
        gl::RenderbufferStorage(gl::RENDERBUFFER, gl::DEPTH_COMPONENT, 1800, 800);
        gl::FramebufferRenderbuffer(gl::FRAMEBUFFER, gl::DEPTH_ATTACHMENT, gl::RENDERBUFFER, depth_rb);

        if gl::CheckFramebufferStatus(gl::FRAMEBUFFER) != gl::FRAMEBUFFER_COMPLETE {
            eprintln!("FATAL: framebuffer status is not complete");
        }
    }

    let info = framebuffer::create(dev, &buff_obj)?;

    Ok(Framebuffer{
        info,
        bo: buff_obj,
        fbo,
    })
}

fn egl_error_msg() -> String {
    match egl::get_error() {
        egl::EGL_NOT_INITIALIZED => "egl not initialized",
        egl::EGL_BAD_ACCESS => "egl bad access",
        egl::EGL_BAD_ALLOC => "egl bad alloc",
        egl::EGL_BAD_ATTRIBUTE => "egl bad attribute",
        egl::EGL_BAD_CONTEXT=> "egl bad context",
        egl::EGL_BAD_CONFIG => "egl bad config",
        egl::EGL_BAD_CURRENT_SURFACE => "egl bad current surface",
        egl::EGL_BAD_DISPLAY => "egl bad display",
        egl::EGL_BAD_SURFACE => "egl bad surface",
        egl::EGL_BAD_MATCH => "egl bad match",
        egl::EGL_BAD_PARAMETER => "egl bad parameter",
        egl::EGL_BAD_NATIVE_PIXMAP => "egl bad native pixmap",
        egl::EGL_BAD_NATIVE_WINDOW => "egl bad native window",
        egl::EGL_CONTEXT_LOST => "egl context lost",
        _ => "unknown error",
    }.to_string()
}

fn load_information<T, U>(card: &Card, handles: &[T]) -> Vec<U>
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

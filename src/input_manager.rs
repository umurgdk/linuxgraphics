use std::collections::HashMap;
use std::os::unix::io::RawFd;
use std::fs::File;
use std::path::Path;
use std::error::Error;
use std::ffi::CStr;
use std::ffi::CString;

use libc;

use dbus;
use dbus::BusType;
use input::LibinputInterface;

pub struct InputManager {
    dbus_conn: dbus::Connection,
    files: HashMap<RawFd, File>,
}

impl InputManager {
    fn new() -> Result<InputManager, Box<Error>> {
        let dbus_conn = dbus::Connection::get_private(BusType::Session)?;
        InputManager { dbus_conn, files: Default::default() }
    }
}

impl LibinputInterface for InputManager {
    fn open_restricted(&mut self, path: &Path, _flags: i32) -> Result<RawFd, i32> {
        println!("[libinput] request open {:?}", path);

        // Find /sys path for the requested device. Libinput gives /dev path but logind can
        // only attach devices from /sys path
        let mut sys_path = "".to_string();
        let p_str = match path.to_str() {
            Some(str) => str,
            None => {
                eprintln!("[inputmanager] not a valid string");
                return Err(-1);
            }
        };


        // Let logind open the device file for us
        let msg = dbus::Message::new_method_call(
            "org.freedesktop.login1",
            "/org/freedesktop/login1",
            "org.freedesktop.login1.Manager",
            "AttachDevice");

        let msg = match msg {
            Ok(msg) => msg,
            Err(err) => {
                eprintln!("[dbus] failed create message: {}", err);
                return Err(-1);
            }
        };

//        msg.append3("seat0", )

        let reply = match self.dbus_conn.send_with_reply_and_block(msg, 5000) {
            Ok(r) => r,
            Err(err) => {
                eprintln!("[dbus] msg send failed: {}", err);
                return Err(-1);
            }
        };


        let file = match File::open(path) {
            Ok(file) => file,
            Err(err) => {
                eprintln!("[libinput] failed to open file {:?}", path);
                return Err(err.raw_os_error().unwrap_or(-1));
            }
        };

        let fd = file.as_raw_fd();
        self.files.insert(fd, file);
        Ok(fd)
    }

    fn close_restricted(&mut self, fd: RawFd) {
        self.files.remove(&fd);
    }
}
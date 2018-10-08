use std::path::Path;
use std::fs::File;
use std::os::unix::io::RawFd;
use std::os::unix::io::AsRawFd;
use std::collections::HashMap;

use input;

pub struct InputInterface {
    files: HashMap<RawFd, File>,
}

impl InputInterface {
    pub fn new() -> InputInterface {
        InputInterface { files: Default::default() }
    }
}

impl input::LibinputInterface for InputInterface {
    fn open_restricted(&mut self, path: &Path, _flags: i32) -> Result<RawFd, i32> {
        println!("[libinput] open {:?}", path);

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
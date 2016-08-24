use std::os::unix::io::RawFd;

use {read, write};
use handler::Handler;
use buf::ByteBuffer;
use error::{Result, Error};
use poll::*;

const BUF_CAP: usize = 1024 * 1024;

/// Handler that echoes incoming bytes
///
/// For benchmarking I/O throuput and latency
pub struct EchoHandler {
    sockw: bool,
    clifd: RawFd,
    buf: ByteBuffer,
}

impl EchoHandler {
    pub fn new(clifd: RawFd) -> EchoHandler {
        trace!("new()");
        EchoHandler {
            clifd: clifd,
            sockw: false,
            buf: ByteBuffer::with_capacity(BUF_CAP),
        }
    }

    fn on_error(&mut self) -> Result<()> {
        error!("on_error()");
        Ok(())
    }

    fn on_readable(&mut self) -> Result<()> {
        trace!("on_readable()");

        if let Some(n) = try!(read(self.clifd, From::from(&mut self.buf))) {
            trace!("on_readable(): {:?} bytes", n);
            self.buf.extend(n);
        } else {
            trace!("on_readable(): socket not ready");
        }

        if self.sockw {
            self.on_writable()
        } else {
            Ok(())
        }
    }

    fn on_writable(&mut self) -> Result<()> {
        trace!("on_writable()");

        if !self.buf.is_empty() {
            if let Some(cnt) = try!(write(self.clifd, From::from(&self.buf))) {
                trace!("on_writable() bytes {}", cnt);
                self.buf.consume(cnt);
            } else {
                trace!("on_writable(): socket not ready");
            }
        } else {
            trace!("on_writable(): empty buf");
            self.sockw = true;
        }
        Ok(())
    }
}

impl Handler for EchoHandler {
    fn is_terminated(&self) -> bool {
        false
    }
    fn ready(&mut self, event: &EpollEvent) {

        let kind = event.events;

        if kind.contains(EPOLLRDHUP) || kind.contains(EPOLLHUP) {
            trace!("socket's fd {}: EPOLLHUP", self.clifd);
            return;
        }

        if kind.contains(EPOLLERR) {
            trace!("socket's fd {}: EPOLERR", self.clifd);
            perror!("on_error()", self.on_error());
        }

        if kind.contains(EPOLLIN) {
            trace!("socket's fd {}: EPOLLIN", self.clifd);
            perror!("on_readable()", self.on_readable());
        }

        if kind.contains(EPOLLOUT) {
            trace!("socket's fd {}: EPOLLOUT", self.clifd);
            perror!("on_writable()", self.on_writable());
        }
    }
}

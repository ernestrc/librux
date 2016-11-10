use std::os::unix::io::RawFd;

use {read, write};
use handler::Handler;
use buf::ByteBuffer;
use error::{Result, Error};
use poll::*;
use protocol::*;

const BUF_CAP: usize = 1024 * 1024;

/// Handler that echoes incoming bytes
///
/// For benchmarking I/O throuput and latency
pub struct EchoHandler<P: IOProtocol> {
    sockw: bool,
    eproto: P,
    buf: ByteBuffer,
}

impl <P: IOProtocol> EchoHandler<P> {
    pub fn new(eproto: P) -> EchoHandler<P> {
        trace!("new()");
        EchoHandler {
            sockw: false,
            eproto: eproto,
            buf: ByteBuffer::with_capacity(BUF_CAP),
        }
    }

    fn on_error(&mut self, fd: RawFd) -> Result<()> {
        error!("on_error(): {:?}", fd);
        Ok(())
    }

    fn on_readable(&mut self, fd: RawFd) -> Result<()> {
        trace!("on_readable()");

        if let Some(n) = try!(read(fd, From::from(&mut self.buf))) {
            trace!("on_readable(): {:?} bytes", n);
            self.buf.extend(n);
        } else {
            trace!("on_readable(): socket not ready");
        }

        if self.sockw {
            self.on_writable(fd)
        } else {
            Ok(())
        }
    }

    fn on_writable(&mut self, fd: RawFd) -> Result<()> {
        trace!("on_writable()");

        if self.buf.is_readable() {
            if let Some(cnt) = try!(write(fd, From::from(&self.buf))) {
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

impl <P: IOProtocol> Handler<EpollEvent> for EchoHandler<P> {

    fn is_terminated(&self) -> bool {
        false
    }

    fn ready(&mut self, event: &EpollEvent) {

        let fd = match self.eproto.decode(event.data) {
            Action::New(_, clifd) => clifd, 
            Action::Notify(_, clifd) => clifd,
            Action::NoAction(data) => data as i32
        };

        let kind = event.events;

        if kind.contains(EPOLLRDHUP) || kind.contains(EPOLLHUP) {
            trace!("socket's fd {}: EPOLLHUP", fd);
            //FIXME sync handler should take a different type of
            //handler !EpollEvent or another trait (i.e. Controller)
            //should be used as a handler responsible of the connection
            ::nix::unistd::close(fd);
            return;
        }

        if kind.contains(EPOLLERR) {
            trace!("socket's fd {}: EPOLERR", fd);
            perror!("on_error()", self.on_error(fd));
        }

        if kind.contains(EPOLLIN) {
            trace!("socket's fd {}: EPOLLIN", fd);
            perror!("on_readable()", self.on_readable(fd));
        }

        if kind.contains(EPOLLOUT) {
            trace!("socket's fd {}: EPOLLOUT", fd);
            perror!("on_writable()", self.on_writable(fd));
        }
    }
}

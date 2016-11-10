#[macro_use] extern crate log;
#[macro_use] extern crate rux;

use std::os::unix::io::RawFd;

use rux::{read, write, close};
use rux::handler::*;
use rux::server::Server;
use rux::logging::SimpleLogging;
use rux::protocol::*;
use rux::poll::*;
use rux::server::mux::*;

use rux::buf::ByteBuffer;

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

    fn on_error(&mut self, fd: RawFd) {
        error!("EPOLLERR: {:?}", fd);
    }

    fn on_readable(&mut self, fd: RawFd) {
        trace!("on_readable()");

        if let Some(n) = read(fd, From::from(&mut self.buf)).unwrap() {
            trace!("on_readable(): {:?} bytes", n);
            self.buf.extend(n);
        } else {
            trace!("on_readable(): socket not ready");
        }

        if self.sockw {
            self.on_writable(fd)
        }
    }

    fn on_writable(&mut self, fd: RawFd) {
        trace!("on_writable()");

        if self.buf.is_readable() {
            if let Some(cnt) = write(fd, From::from(&self.buf)).unwrap() {
                trace!("on_writable() bytes {}", cnt);
                self.buf.consume(cnt);
            } else {
                trace!("on_writable(): socket not ready");
            }
        } else {
            trace!("on_writable(): empty buf");
            self.sockw = true;
        }
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
            close(fd);
            return;
        }

        if kind.contains(EPOLLERR) {
            trace!("socket's fd {}: EPOLERR", fd);
            self.on_error(fd);
        }

        if kind.contains(EPOLLIN) {
            trace!("socket's fd {}: EPOLLIN", fd);
            self.on_readable(fd);
        }

        if kind.contains(EPOLLOUT) {
            trace!("socket's fd {}: EPOLLOUT", fd);
            self.on_writable(fd);
        }
    }
}

#[derive(Clone, Copy)]
struct EchoProtocol;

impl StaticProtocol<EchoHandler<EchoProtocol>> for EchoProtocol {

    fn get_handler(&self, _: usize, _: EpollFd, _: usize) -> EchoHandler<EchoProtocol> {
        EchoHandler::new(EchoProtocol)
    }
}

impl IOProtocol for EchoProtocol {

    type Protocol = usize;
}

fn main() {

    let config = MuxConfig::new(("127.0.0.1", 10003))
        .unwrap()
        .io_threads(6);

    let logging = SimpleLogging::new(::log::LogLevel::Info);

    Server::bind(Mux::new(config, EchoProtocol).unwrap(), logging).unwrap();

}

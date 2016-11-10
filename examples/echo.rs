#[macro_use]
extern crate log;
#[macro_use]
extern crate rux;
extern crate slab;

use std::os::unix::io::RawFd;

use slab::Slab;
use rux::{read, write, close};
use rux::handler::*;
use rux::server::Server;
use rux::logging::SimpleLogging;
use rux::protocol::*;
use rux::poll::*;
use rux::error::*;
use rux::sys::socket::*;
use rux::server::mux::*;
use rux::buf::ByteBuffer;

const BUF_CAP: usize = 1024 * 1024;

/// Handler that echoes incoming bytes
///
/// For benchmarking I/O throuput and latency
pub struct EchoHandler {
    sockw: bool,
    epfd: EpollFd,
    connections: Slab<RawFd, usize>,
    buf: ByteBuffer,
}

impl EchoHandler {
    pub fn new(epfd: EpollFd) -> EchoHandler {
        trace!("new()");
        EchoHandler {
            epfd: epfd,
            sockw: false,
            connections: Slab::with_capacity(10_000),
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

impl Handler<EpollEvent> for EchoHandler {
    fn is_terminated(&self) -> bool {
        false
    }

    fn ready(&mut self, event: &EpollEvent) {

        let fd = match EchoProtocol.decode(event.data) {
            Action::New(_, clifd) => clifd, 
            Action::Notify(_, clifd) => clifd,
            Action::NoAction(data) => {
                let srvfd = data as i32;
                // only monitoring events from srvfd
                match eintr!(accept4, "accept4", srvfd, SOCK_NONBLOCK) {
                    Ok(Some(clifd)) => {

                        trace!("accept4: accepted new tcp client {}", &clifd);

                        debug!("assigned accepted client {}; epoll instance {}",
                               &clifd,
                               &self.epfd);

                        match self.connections.vacant_entry() {
                            Some(entry) => {

                                let i = entry.index();
                                entry.insert(clifd);

                                let action = Action::Notify(i, clifd);
                                let interest = EpollEvent {
                                    events: EPOLLIN | EPOLLOUT | EPOLLHUP | EPOLLRDHUP | EPOLLET,
                                    data: EchoProtocol.encode(action),
                                };

                                match self.epfd.register(clifd, &interest) {
                                    Ok(_) => {}
                                    Err(e) => {
                                        error!("closing: {:?}", e);
                                        perror!("{}", close(clifd));
                                    }
                                };
                                trace!("epoll_ctl: registered interests for {}", clifd);
                            }
                            None => error!("connection slab full"),
                        }
                    }
                    Ok(None) => error!("accept4: socket not ready"),
                    Err(e) => {
                        error!("accept4: {}", e);
                    }
                };
                return;
            }
        };

        let kind = event.events;

        if kind.contains(EPOLLRDHUP) || kind.contains(EPOLLHUP) {
            trace!("socket's fd {}: EPOLLHUP", fd);
            perror!("close: {}", close(fd));
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

impl StaticProtocol<EchoHandler> for EchoProtocol {
    fn get_handler(&self, _: usize, epfd: EpollFd, _: usize) -> EchoHandler {
        EchoHandler::new(epfd)
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

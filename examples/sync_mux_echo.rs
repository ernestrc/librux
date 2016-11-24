#[macro_use]
extern crate log;
#[macro_use]
extern crate rux;
extern crate env_logger;

use rux::{read, send as rsend, close};
use rux::handler::*;
use rux::protocol::*;
use rux::poll::*;
use rux::error::*;
use rux::sys::socket::*;
use rux::prop::server::*;
use rux::handler::mux::{SyncMux, MuxEvent, MuxCmd};
use rux::prop::system::System;

const BUF_SIZE: usize = 1024;
const EPOLL_BUF_SIZE: usize = 100;
const EPOLL_LOOP_MS: isize = 100;
const MAX_CONN: usize = 24 * 1024;

/// Handler that echoes incoming bytes
///
/// For benchmarking I/O throuput and latency
pub struct EchoHandler;

impl Handler for EchoHandler {
    type In = MuxEvent;
    type Out = MuxCmd;

    fn ready(&mut self, event: MuxEvent) -> MuxCmd {

        let fd = event.fd;
        let kind = event.kind;
        let buf = event.buffer();

        if kind.contains(EPOLLHUP) {
            trace!("socket's fd {}: EPOLLHUP", fd);
            perror!("close: {}", close(fd));
            return MuxCmd::Clear;
        }

        if kind.contains(EPOLLERR) {
            error!("socket's fd {}: EPOLERR", fd);
        }

        if kind.contains(EPOLLIN) {
            trace!("socket's fd {}: EPOLLIN", fd);
            if let Some(n) = read(fd, From::from(&mut *buf)).unwrap() {
                trace!("on_readable(): {:?} bytes", n);
                buf.extend(n);
            } else {
                trace!("on_readable(): socket not ready");
            }
        }

        if kind.contains(EPOLLOUT) {
            trace!("socket's fd {}: EPOLLOUT", fd);
            if buf.is_readable() {
                if let Some(cnt) = rsend(fd, From::from(&*buf), MSG_DONTWAIT).unwrap() {
                    trace!("on_writable() bytes {}", cnt);
                    buf.consume(cnt);
                } else {
                    trace!("on_writable(): socket not ready");
                }
            }
        }

        MuxCmd::Keep
    }
}

#[derive(Clone)]
struct EchoProtocol;

impl<'p> StaticProtocol<'p, MuxEvent, MuxCmd> for EchoProtocol {
    type H = EchoHandler;

    fn get_handler(&self, _: Position<usize>, _: EpollFd, _: usize) -> EchoHandler {
        EchoHandler {}
    }
}

impl<'p> StaticProtocol<'p, EpollEvent, EpollCmd> for EchoProtocol {
    type H = SyncMux<'p, EchoProtocol>;

    fn get_handler(&'p self,
                   _: Position<usize>,
                   epfd: EpollFd,
                   _: usize)
                   -> SyncMux<'p, EchoProtocol> {

        let interests = EPOLLIN | EPOLLOUT | EPOLLET;
        SyncMux::new(BUF_SIZE, MAX_CONN, interests, epfd, &self)
    }
}

impl MuxProtocol for EchoProtocol {
    type Protocol = usize;
}

fn main() {

    ::env_logger::init().unwrap();

    let config = ServerConfig::new(("127.0.0.1", 10002))
        .unwrap()
        .io_threads(4)
        .epoll_config(EpollConfig {
            loop_ms: EPOLL_LOOP_MS,
            buffer_size: EPOLL_BUF_SIZE,
        });

    let protocol = EchoProtocol;

    System::build(Server::new(config).unwrap()).start(&protocol).unwrap();
}

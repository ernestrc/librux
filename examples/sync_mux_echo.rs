#[macro_use]
extern crate log;
#[macro_use]
extern crate rux;

use rux::{read, send as rsend, close};
use rux::handler::*;
use rux::prop::Prop;
use rux::logging::SimpleLogging;
use rux::protocol::*;
use rux::poll::*;
use rux::error::*;
use rux::sys::socket::*;
use rux::prop::server::*;
use rux::handler::mux::{SSyncMux, MuxEvent};

const BUF_SIZE: usize = 1024;
const EPOLL_BUF_SIZE: usize = 100;
const EPOLL_LOOP_MS: isize = -1;
const MAX_CONN: usize = 24 * 1024;

/// Handler that echoes incoming bytes
///
/// For benchmarking I/O throuput and latency
pub struct EchoHandler;

impl Handler for EchoHandler {
    type In = MuxEvent;
    type Out = ();

    fn reset(&mut self) { }

    fn ready(&mut self, event: MuxEvent) -> Option<()> {

        let fd = event.fd;
        let kind = event.kind;
        let buf = event.buffer();

        if kind.contains(EPOLLHUP) {
            trace!("socket's fd {}: EPOLLHUP", fd);
            perror!("close: {}", close(fd));
            return Some(());
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

        None
    }
}

#[derive(Clone, Copy)]
struct EchoProtocol;

impl StaticProtocol<MuxEvent, ()> for EchoProtocol {
    type H = EchoHandler;

    fn get_handler(&self, _: usize, _: EpollFd, _: usize) -> EchoHandler {
        EchoHandler {}
    }
}

impl StaticProtocol<EpollEvent, ()> for EchoProtocol {

    type H = SSyncMux<EchoProtocol>;

    fn get_handler(&self,
                   _: usize,
                   epfd: EpollFd,
                   _: usize)
                   -> SSyncMux<EchoProtocol> {
        SSyncMux::new(BUF_SIZE, MAX_CONN, epfd, EchoProtocol)
    }
}

impl MuxProtocol for EchoProtocol {
    type Protocol = usize;
}

fn main() {

    let config = ServerConfig::new(("127.0.0.1", 10002))
        .unwrap()
        .io_threads(1)
        .epoll_config(EpollConfig {
            loop_ms: EPOLL_LOOP_MS,
            buffer_size: EPOLL_BUF_SIZE,
        });

    let logging = SimpleLogging::new(::log::LogLevel::Debug);

    Prop::create(Server::new(config, EchoProtocol).unwrap(), logging).unwrap();
}

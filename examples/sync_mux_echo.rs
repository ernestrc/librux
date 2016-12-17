#[macro_use]
extern crate log;
#[macro_use]
extern crate rux;
extern crate env_logger;

use rux::{read, send as rsend, close};
use rux::handler::*;
use rux::protocol::*;
use rux::poll::*;
use rux::buf::ByteBuffer;
use rux::error::*;
use rux::sys::socket::*;
use rux::prop::server::*;
use rux::handler::mux::{SyncMux, MuxEvent, MuxCmd};
use rux::prop::system::System;

const BUF_SIZE: usize = 2048;
const EPOLL_BUF_SIZE: usize = 2048;
const EPOLL_LOOP_MS: isize = -1;
const MAX_CONN: usize = 2048;

/// Handler that echoes incoming bytes
///
/// For benchmarking I/O throuput and latency
pub struct EchoHandler {
    buffer: ByteBuffer
}

impl Handler for EchoHandler {
    type In = MuxEvent;
    type Out = MuxCmd;

    fn ready(&mut self, event: MuxEvent) -> MuxCmd {

        let fd = event.fd;
        let kind = event.kind;

        if kind.contains(EPOLLHUP) || kind.contains(EPOLLRDHUP) {
            trace!("socket's fd {}: EPOLLHUP", fd);
            perror!("close: {}", close(fd));
            return MuxCmd::Clear;
        }

        if kind.contains(EPOLLERR) {
            error!("socket's fd {}: EPOLERR", fd);
        }

        if kind.contains(EPOLLIN) {
            trace!("socket's fd {}: EPOLLIN", fd);
            if let Some(n) = read(fd, From::from(&mut self.buffer)).unwrap() {
                trace!("on_readable(): {:?} bytes", n);
                self.buffer.extend(n);
            } else {
                trace!("on_readable(): socket not ready");
            }
        }

        if kind.contains(EPOLLOUT) {
            trace!("socket's fd {}: EPOLLOUT", fd);
            if self.buffer.is_readable() {
                if let Some(cnt) = rsend(fd, From::from(&self.buffer), MSG_DONTWAIT).unwrap() {
                    trace!("on_writable() bytes {}", cnt);
                    self.buffer.consume(cnt);
                } else {
                    trace!("on_writable(): socket not ready");
                }
            }
        }

        MuxCmd::Keep
    }
}

#[derive(Clone, Debug)]
struct EchoProtocol {
    buffers: Vec<Option<ByteBuffer>>
}

impl<'p> StaticProtocol<'p, MuxEvent, MuxCmd> for EchoProtocol {
    type H = EchoHandler;

    fn done(&mut self, handler: EchoHandler, index: usize) {
        let EchoHandler { mut buffer } = handler;

        buffer.clear();

        trace!("done(): {:?}", self.buffers);
        self.buffers[index] = Some(buffer);
        trace!("done(): {:?}", self.buffers);
    }

    fn get_handler(&mut self, _: Position<usize>, _: EpollFd, index: usize) -> EchoHandler {
        trace!("get_handler(): {:p}", &self.buffers);
        let buffer = self.buffers[index].take().unwrap();
        trace!("get_handler(): {:?}", self.buffers);
        EchoHandler { buffer: buffer }
    }
}

impl<'p> StaticProtocol<'p, EpollEvent, EpollCmd> for EchoProtocol {
    type H = SyncMux<'p, EchoProtocol>;

    fn done(&mut self, _: SyncMux<'p, EchoProtocol>, _: usize) {}

    fn get_handler(&'p mut self,
                   _: Position<usize>,
                   epfd: EpollFd,
                   _: usize)
                   -> SyncMux<'p, EchoProtocol> {
        let interests = EPOLLIN | EPOLLOUT | EPOLLET | EPOLLRDHUP;
        SyncMux::new(MAX_CONN, interests, epfd, self)
    }
}

impl MuxProtocol for EchoProtocol {
    type Protocol = usize;
}

fn main() {

    ::env_logger::init().unwrap();

    info!("BUF_SIZE: {}; EPOLL_BUF_SIZE: {}; EPOLL_LOOP_MS: {}; MAX_CONN: {}",
          BUF_SIZE,
          EPOLL_BUF_SIZE,
          EPOLL_LOOP_MS,
          MAX_CONN);

    let config = ServerConfig::new(("127.0.0.1", 10001))
        .unwrap()
        .max_conn(MAX_CONN)
        .io_threads(1)
        .epoll_config(EpollConfig {
            loop_ms: EPOLL_LOOP_MS,
            buffer_size: EPOLL_BUF_SIZE,
        });

    let buffers = vec!(ByteBuffer::with_capacity(BUF_SIZE); MAX_CONN);

    for buffer in buffers.iter() {
        assert!(buffer.capacity() == BUF_SIZE);
    }

    assert!(buffers.len() == MAX_CONN);

    let protocol = EchoProtocol {
        buffers: buffers.into_iter().map(|b| Some(b)).collect(),
    };

    System::build(Server::new(config, protocol).unwrap()).start().unwrap();
}

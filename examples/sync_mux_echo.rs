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
pub struct EchoHandler<'p> {
    buffer: &'p mut ByteBuffer,
}

impl<'p> Handler for EchoHandler<'p> {
    type In = MuxEvent;
    type Out = MuxCmd;

    fn update(&mut self, _: EpollFd) {}

    fn ready(&mut self, event: MuxEvent) -> MuxCmd {

        let fd = event.fd;
        let kind = event.kind;

        if kind.contains(EPOLLHUP) || kind.contains(EPOLLRDHUP) {
            perror!("close: {}", close(fd));
            return MuxCmd::Clear;
        }

        if kind.contains(EPOLLERR) {
            error!("socket's fd {}: EPOLERR", fd);
        }

        if kind.contains(EPOLLIN) {
            if let Some(n) = read(fd, From::from(&mut *self.buffer)).unwrap() {
                self.buffer.extend(n);
            }
        }

        if kind.contains(EPOLLOUT) {
            if self.buffer.is_readable() {
                if let Some(cnt) = rsend(fd, From::from(&*self.buffer), MSG_DONTWAIT).unwrap() {
                    self.buffer.consume(cnt);
                }
            }
        }

        MuxCmd::Keep
    }
}

#[derive(Clone, Debug)]
struct EchoProtocol {
    buffers: Vec<ByteBuffer>,
}

impl<'p> StaticProtocol<'p, MuxEvent, MuxCmd> for EchoProtocol {
    type H = EchoHandler<'p>;

    fn done(&mut self, handler: EchoHandler, _: usize) {
        handler.buffer.clear();
    }

    fn get_handler(&'p mut self, _: Position<usize>, _: EpollFd, index: usize) -> EchoHandler<'p> {
        EchoHandler { buffer: &mut self.buffers[index] }
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

    let config = ServerConfig::tcp(("127.0.0.1", 10001))
        .unwrap()
        .max_conn(MAX_CONN)
        .io_threads(1)
        .epoll_config(EpollConfig {
            loop_ms: EPOLL_LOOP_MS,
            buffer_size: EPOLL_BUF_SIZE,
        });

    let protocol = EchoProtocol { buffers: vec!(ByteBuffer::with_capacity(BUF_SIZE); MAX_CONN) };

    let server = Server::new_with(config, |epfd| {
            SyncMux::new(MAX_CONN,
                         EPOLLIN | EPOLLOUT | EPOLLET | EPOLLRDHUP,
                         epfd,
                         protocol)
        })
        .unwrap();

    System::build(server).start().unwrap();
}

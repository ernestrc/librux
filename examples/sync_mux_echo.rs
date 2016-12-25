#[macro_use]
extern crate log;
#[macro_use]
extern crate rux;
extern crate env_logger;

use rux::{send as rsend, recv as rrecv};
use rux::buf::ByteBuffer;
use rux::handler::*;
use rux::mux::*;
use rux::poll::*;
use rux::prop::server::*;
use rux::sys::socket::*;
use rux::system::System;

const BUF_SIZE: usize = 2048;
const EPOLL_BUF_CAP: usize = 2048;
const EPOLL_LOOP_MS: isize = -1;
const MAX_CONN: usize = 2048;

/// Handler that echoes incoming bytes
///
/// For benchmarking I/O throuput and latency
pub struct EchoHandler<'p> {
  buffer: &'p mut ByteBuffer,
}

impl<'p> Handler<EpollEvent, MuxCmd> for EchoHandler<'p> {
  fn ready(&mut self, event: EpollEvent) -> MuxCmd {

    let fd = event.data as i32;
    let kind = event.events;

    if kind.contains(EPOLLHUP) || kind.contains(EPOLLRDHUP) {
      trace!("socket's fd {}: EPOLLHUP", fd);
      return MuxCmd::Close;
    }

    if kind.contains(EPOLLERR) {
      error!("socket's fd {}: EPOLERR", fd);
    }

    if kind.contains(EPOLLIN) {
      if let Some(n) = rrecv(fd, From::from(&mut *self.buffer), MSG_DONTWAIT).unwrap() {
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

impl<'p> HandlerFactory<'p, EpollEvent, MuxCmd> for EchoProtocol {
  type H = EchoHandler<'p>;

  fn done(&mut self, handler: EchoHandler, _: usize) {
    handler.buffer.clear();
  }

  fn new(&'p mut self, _: EpollFd, index: usize) -> EchoHandler<'p> {
    EchoHandler { buffer: &mut self.buffers[index] }
  }
}

fn main() {

  ::env_logger::init().unwrap();

  info!("BUF_SIZE: {}; EPOLL_BUF_CAP: {}; EPOLL_LOOP_MS: {}; MAX_CONN: {}",
        BUF_SIZE,
        EPOLL_BUF_CAP,
        EPOLL_LOOP_MS,
        MAX_CONN);

  let config = ServerConfig::tcp(("127.0.0.1", 10001))
    .unwrap()
    .max_conn(MAX_CONN)
    .io_threads(1)
    .epoll_config(EpollConfig {
      loop_ms: EPOLL_LOOP_MS,
      buffer_capacity: EPOLL_BUF_CAP,
    });

  let protocol = EchoProtocol { buffers: vec!(ByteBuffer::with_capacity(BUF_SIZE); MAX_CONN) };

  let server = Server::new_with(config, |epfd| {
      SyncMux::new(MAX_CONN, EPOLLIN | EPOLLOUT | EPOLLET | EPOLLRDHUP, epfd, protocol)
    })
    .unwrap();

  System::build(server).start().unwrap();
}

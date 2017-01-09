#[macro_use]
extern crate log;
#[macro_use]
extern crate rux;
extern crate num_cpus;
extern crate env_logger;

use rux::{send as rsend, recv as rrecv};
use rux::{RawFd, Reset};
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
pub struct EchoHandler;

impl<'a> Handler<MuxEvent<'a, ByteBuffer>, MuxCmd> for EchoHandler {
  fn on_next(&mut self, event: MuxEvent<'a, ByteBuffer>) -> MuxCmd {

    let fd = event.fd;
    let kind = event.kind;
    let buffer = event.resource;

    if kind.contains(EPOLLHUP) {
      trace!("socket's fd {}: EPOLLHUP", fd);
      return MuxCmd::Close;
    }

    if kind.contains(EPOLLERR) {
      error!("socket's fd {}: EPOLERR", fd);
      return MuxCmd::Close;
    }

    if kind.contains(EPOLLIN) {
      if let Some(n) = rrecv(fd, From::from(&mut *buffer), MSG_DONTWAIT).unwrap() {
        buffer.extend(n);
      }
    }

    if kind.contains(EPOLLOUT) {
      if buffer.is_readable() {
        if let Some(cnt) = rsend(fd, From::from(&*buffer), MSG_DONTWAIT).unwrap() {
          buffer.consume(cnt);
        }
      }
    }

    MuxCmd::Keep
  }
}

impl EpollHandler for EchoHandler {
  fn interests() -> EpollEventKind {
    EPOLLIN | EPOLLOUT | EPOLLET
  }

  fn with_epfd(&mut self, _: EpollFd) {

  }
}

impl Reset for EchoHandler {
  fn reset(&mut self) {}
}

#[derive(Clone, Debug)]
struct EchoFactory;

impl<'a> HandlerFactory<'a, EchoHandler, ByteBuffer> for EchoFactory {

  fn new_resource(&self) -> ByteBuffer {
    ByteBuffer::with_capacity(BUF_SIZE)
  }

  fn new_handler(&mut self, _: EpollFd, _: RawFd) -> EchoHandler {
    EchoHandler
  }
}

fn main() {

  ::env_logger::init().unwrap();

  info!("BUF_SIZE: {}; EPOLL_BUF_CAP: {}; EPOLL_LOOP_MS: {}; MAX_CONN: {}",
        BUF_SIZE,
        EPOLL_BUF_CAP,
        EPOLL_LOOP_MS,
        MAX_CONN);

  let config = ServerConfig::tcp(("127.0.0.1", 9999))
    .unwrap()
    .max_conn(MAX_CONN)
    .io_threads(::std::cmp::max(1, ::num_cpus::get() / 2))
    // .io_threads(1)
    .epoll_config(EpollConfig {
      loop_ms: EPOLL_LOOP_MS,
      buffer_capacity: EPOLL_BUF_CAP,
    });

  let server = Server::new_with(config, |epfd| {
      SyncMux::new(MAX_CONN, epfd, EchoFactory)
    })
    .unwrap();

  System::build(server).start().unwrap();
}

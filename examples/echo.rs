// #[macro_use]
// extern crate log;
// #[macro_use]
// extern crate rux;
// 
// use std::os::unix::io::RawFd;
// 
// use rux::{read, send as rsend, close};
// use rux::handler::*;
// use rux::prop::Prop;
// use rux::logging::SimpleLogging;
// use rux::protocol::*;
// use rux::poll::*;
// use rux::error::*;
// use rux::sys::socket::*;
// use rux::prop::server::*;
// use rux::buf::ByteBuffer;
// 
// const BUF_SIZE: usize = 1024;
// const EPOLL_BUF_SIZE: usize = 100;
// const EPOLL_LOOP_MS: isize = -1;
// 
// /// Handler that echoes incoming bytes
// ///
// /// For benchmarking I/O throuput and latency
// pub struct EchoHandler {
//     epfd: EpollFd,
//     buf: ByteBuffer,
// }
// 
// impl EchoHandler {
//     pub fn new(epfd: EpollFd) -> EchoHandler {
//         trace!("new()");
//         EchoHandler {
//             epfd: epfd,
//             buf: ByteBuffer::with_capacity(BUF_SIZE),
//         }
//     }
// 
//     fn on_error(&mut self, fd: RawFd) {
//         error!("EPOLLERR: {:?}", fd);
//     }
// 
//     fn on_readable(&mut self, fd: RawFd) {
//         trace!("on_readable()");
// 
//         if let Some(n) = read(fd, From::from(&mut self.buf)).unwrap() {
//             trace!("on_readable(): {:?} bytes", n);
//             self.buf.extend(n);
//         } else {
//             trace!("on_readable(): socket not ready");
//         }
//     }
// 
//     fn on_writable(&mut self, fd: RawFd) {
//         trace!("on_writable()");
// 
//         if self.buf.is_readable() {
//             if let Some(cnt) = rsend(fd, From::from(&self.buf), MSG_DONTWAIT).unwrap() {
//                 trace!("on_writable() bytes {}", cnt);
//                 self.buf.consume(cnt);
//             } else {
//                 trace!("on_writable(): socket not ready");
//             }
//         }
//     }
// }
// 
// impl Handler for EchoHandler {
//     type In = EpollEvent;
//     type Out = ();
// 
//     fn reset(&mut self) { }
// 
//     fn ready(&mut self, event: EpollEvent) -> Option<()> {
// 
//         let fd = match EchoProtocol.decode(event.data) {
//             Action::New(_, clifd) => clifd, 
//             Action::Notify(_, clifd) => clifd,
//             Action::NoAction(data) => {
//                 let srvfd = data as i32;
//                 // only monitoring events from srvfd
//                 match eintr!(accept4, "accept4", srvfd, SockFlag::empty()) {
//                     Ok(Some(clifd)) => {
// 
//                         trace!("accept4: accepted new tcp client {} in epoll instance {}",
//                                &clifd,
//                                &self.epfd);
// 
//                         let action = Action::Notify(0, clifd);
//                         let interest = EpollEvent {
//                             events: EPOLLIN | EPOLLOUT | EPOLLET,
//                             data: EchoProtocol.encode(action),
//                         };
// 
//                         match self.epfd.register(clifd, &interest) {
//                             Ok(_) => {}
//                             Err(e) => {
//                                 error!("register: {:?}", e);
//                                 perror!("close: {}", close(clifd));
//                             }
//                         };
//                         trace!("epoll_ctl: registered interests for {}", clifd);
//                     }
//                     Ok(None) => error!("accept4: socket not ready"),
//                     Err(e) => {
//                         error!("accept4: {}", e);
//                     }
//                 };
//                 return None;
//             }
//         };
// 
//         let kind = event.events;
// 
//         if kind.contains(EPOLLHUP) {
//             trace!("socket's fd {}: EPOLLHUP", fd);
//             perror!("close: {}", close(fd));
//             return None;
//         }
// 
//         if kind.contains(EPOLLERR) {
//             trace!("socket's fd {}: EPOLERR", fd);
//             self.on_error(fd);
//         }
// 
//         if kind.contains(EPOLLIN) {
//             trace!("socket's fd {}: EPOLLIN", fd);
//             self.on_readable(fd);
//         }
// 
//         if kind.contains(EPOLLOUT) {
//             trace!("socket's fd {}: EPOLLOUT", fd);
//             self.on_writable(fd);
//         }
// 
//         None
//     }
// }
// 
// #[derive(Clone)]
// struct EchoProtocol;
// 
// impl <'p> StaticProtocol<'p, EpollEvent, ()> for EchoProtocol {
// 
//     type H = EchoHandler;
// 
//     fn get_handler(&self, _: Position<usize>, epfd: EpollFd, _: usize) -> EchoHandler {
//         EchoHandler::new(epfd)
//     }
// }
// 
// impl MuxProtocol for EchoProtocol {
//     type Protocol = usize;
// }
// 
fn main() {

    //let config = ServerConfig::new(("127.0.0.1", 10002))
    //    .unwrap()
    //    .io_threads(1)
    //    .epoll_config(EpollConfig {
    //        loop_ms: EPOLL_LOOP_MS,
    //        buffer_size: EPOLL_BUF_SIZE,
    //    });

    //let logging = SimpleLogging::new(::log::LogLevel::Debug);

    //Prop::create(Server::new(config, EchoProtocol).unwrap(), logging).unwrap();
}

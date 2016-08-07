#![feature(split_off, alloc_system)]
extern crate alloc_system;
extern crate log;
#[macro_use] extern crate librux;

use librux::*;
use librux::handler::echo::EchoHandler;
use librux::server::simplemux::*;

#[derive(Clone, Copy)]
struct EchoProtocol;

impl EpollProtocol for EchoProtocol {

    type Protocol = usize;

    fn new(&self, _: usize, fd: RawFd, epfd: EpollFd) -> Box<Handler> {
        Box::new(EchoHandler::new(fd))
    }
}

fn main() {

    let config = SimpleMuxConfig::new(("127.0.0.1", 10003)).unwrap();

    let logging = SimpleLogging::new(::log::LogLevel::Info);

    Server::bind(SimpleMux::new(EchoProtocol, config).unwrap(), logging).unwrap();

}

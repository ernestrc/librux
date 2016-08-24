extern crate log;
#[macro_use] extern crate rux;

use rux::*;
use rux::handler::echo::EchoHandler;
use rux::server::simplemux::*;

#[derive(Clone, Copy)]
struct EchoProtocol;

impl IOProtocol for EchoProtocol {

    type Protocol = usize;

    fn on_new_fd(&self, _: usize, fd: RawFd, epfd: EpollFd) -> Box<Handler> {
        Box::new(EchoHandler::new(fd))
    }
}

fn main() {

    let config = SimpleMuxConfig::new(("127.0.0.1", 10003)).unwrap();

    let logging = SimpleLogging::new(::log::LogLevel::Info);

    Server::bind(SimpleMux::new(EchoProtocol, config).unwrap(), logging).unwrap();

}

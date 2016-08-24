extern crate log;
#[macro_use]
extern crate rux;

use rux::*;
use rux::handler::echo::EchoHandler;
use rux::server::simplemux::*;

#[derive(Clone, Copy)]
struct EchoProtocol;

impl IOProtocol for EchoProtocol {
    type Protocol = usize;

    fn get_handler(&self, _: Self::Protocol, fd: RawFd, epfd: EpollFd) -> Box<Handler> {
        if fd == epfd.fd {
            Box::new(SyncHandler::new(epfd, EchoProtocol, 1000))
        } else {
            Box::new(EchoHandler::new(fd))
        }
    }
}

fn main() {

    let config = SimpleMuxConfig::new(("127.0.0.1", 10003)).unwrap();

    let logging = SimpleLogging::new(::log::LogLevel::Info);

    Server::bind(SimpleMux::new(config, EchoProtocol).unwrap(), logging).unwrap();

}

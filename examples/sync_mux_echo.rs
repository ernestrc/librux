/*
extern crate log;
#[macro_use]
extern crate rux;

use rux::*;
use rux::handler::echo::EchoHandler;
use rux::server::simplemux::{SimpleMuxConfig, SimpleMux};

#[derive(Clone, Copy)]
struct EchoProtocol;

impl DynamicProtocol for EchoProtocol {
    fn get_handler(&self, p: Self::Protocol, epfd: EpollFd, id: usize) -> Box<Handler<EpollEvent>> {
        if p == 0 {
            Box::new(SyncHandler::new(epfd, id, EchoProtocol, 1, 10000))
        } else {
            Box::new(EchoHandler::new(EchoProtocol))
        }
    }
}

impl IOProtocol for EchoProtocol {
    type Protocol = usize;
}
*/

fn main() {

    // let config = SimpleMuxConfig::new(("127.0.0.1", 10003))
    //     .unwrap()
    //     .io_threads(6);

    // let logging = SimpleLogging::new(::log::LogLevel::Info);

    // Server::bind(SimpleMux::new(config, EchoProtocol).unwrap(), logging).unwrap();

}

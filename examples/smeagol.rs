#[macro_use] extern crate log;
#[macro_use] extern crate rux;
extern crate httparse;

mod handler;

use handler::{SmeagolHandler, SmeagolProto};
use rux::{Handler, EpollEvent, EpollFd, IOProtocol, SimpleLogging, Server};
use rux::server::simplemux::*;

fn main() {

    let config = SimpleMuxConfig::new(("127.0.0.1", 10003))
        .unwrap()
        .io_threads(6);

    let logging = SimpleLogging::new(::log::LogLevel::Info);

    Server::bind(SimpleMux::new(config, SmeagolProto).unwrap(), logging).unwrap();

}

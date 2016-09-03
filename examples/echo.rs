extern crate log;
#[macro_use] extern crate rux;

use rux::*;
use rux::handler::echo::EchoHandler;
use rux::server::simplemux::*;

#[derive(Clone, Copy)]
struct EchoProtocol;

impl IOProtocol for EchoProtocol {

    type Protocol = usize;

    fn get_handler(&self, _: usize, _: EpollFd) -> Box<Handler<EpollEvent>> {
        Box::new(EchoHandler::new(EchoProtocol))
    }
}

//TODO fix
//thread 'thread '<unnamed><unnamed>' panicked at '' panicked at 'cannot access stdout during shutdowncannot access stdout during shutdown', ',
fn main() {

    let config = SimpleMuxConfig::new(("127.0.0.1", 10003))
        .unwrap()
        .io_threads(1)
        .loop_ms(-1);

    let logging = SimpleLogging::new(::log::LogLevel::Info);

    Server::bind(SimpleMux::new(config, EchoProtocol).unwrap(), logging).unwrap();

}

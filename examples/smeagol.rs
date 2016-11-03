#[macro_use]
extern crate log;
#[macro_use]
extern crate rux;
extern crate httparse;

mod handler_2;

use std::path::Path;

use handler_2::SmeagolHandler;
use rux::{Handler, EpollEvent, EpollFd, IOProtocol, SimpleLogging, Server, SyncHandler};
use rux::server::simplemux::*;

static MAX_CONN: &'static usize = &10000;
static LOGDIR: &'static str = "/tmp/smeagol";
static IBUFFER: usize = 8192;
static OBUFFER: usize = 16 * 1024 * 1024;
static BUFFERING: usize = 8 * 1024;
static LOOP_MS: isize = -1;

#[derive(Clone, Copy)]
struct Smeagol;

impl IOProtocol for Smeagol {
    type Protocol = usize;

    fn get_handler(&self, _: Self::Protocol, epfd: EpollFd, id: usize) -> Box<Handler<EpollEvent>> {
        let raw = format!("{}/{}", LOGDIR, id);
        let elogdir = Path::new(&raw);

        match ::std::fs::metadata(&elogdir) {
            Ok(ref cfg_attr) if cfg_attr.is_dir() => info!("log dir found {:?}", elogdir),
            _ => {
                info!("creating {:?}", &elogdir);
                ::std::fs::create_dir_all(elogdir)
                    .expect(&format!("could not create {:?}", &elogdir));
            }
        };
        Box::new(SmeagolHandler::new(LOGDIR, id, IBUFFER, OBUFFER, BUFFERING, *MAX_CONN, epfd))
    }
}

fn main() {

    let config = SimpleMuxConfig::new(("127.0.0.1", 10003))
        .unwrap()
        .loop_ms(LOOP_MS)
        .max_conn(*MAX_CONN);

    let logging = SimpleLogging::new(::log::LogLevel::Info);

    Server::bind(SimpleMux::new(config, Smeagol).unwrap(), logging).unwrap();
}

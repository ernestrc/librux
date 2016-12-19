use error::Result;
use handler::Handler;
use poll::{EpollEvent, EpollCmd, Epoll, EpollConfig};

pub mod system;
pub mod server;
pub mod signals;

use self::signals::SigSet;

pub trait Prop {
    type EpollHandler: Handler<EpollEvent, EpollCmd>;

    fn get_epoll_config(&self) -> EpollConfig;

    fn stop(&self);

    fn setup(&mut self, mask: SigSet) -> Result<Epoll<Self::EpollHandler>>;
}

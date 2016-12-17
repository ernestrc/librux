use error::Result;
use poll::{EpollEvent, EpollCmd, Epoll, EpollConfig};
use handler::Handler;

pub mod system;
pub mod server;
pub mod signals;

use self::signals::SigSet;

pub trait Prop {
    type Root: Handler<In=EpollEvent, Out=EpollCmd>;

    fn get_epoll_config(&self) -> EpollConfig;

    fn stop(&self);

    fn setup(&mut self, mask: SigSet) -> Result<Epoll<Self::Root>>;
}

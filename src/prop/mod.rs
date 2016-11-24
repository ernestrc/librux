use error::Result;
use poll::{EpollEvent, EpollCmd, Epoll, EpollConfig};
use protocol::StaticProtocol;

pub mod system;
pub mod server;
pub mod signals;

use self::signals::SigSet;

pub trait Prop {
    fn get_epoll_config(&self) -> EpollConfig;

    fn stop(&self);

    fn setup<'p, P>(&mut self, mask: SigSet, protocol: &'p P) -> Result<Epoll<P::H>>
        where P: StaticProtocol<'p, EpollEvent, EpollCmd>;
}

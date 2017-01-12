
use epoll::{EpollEvent, EpollCmd, Epoll, EpollConfig};
use error::Result;
use handler::Handler;

pub mod server;
pub mod signals;

use self::signals::SigSet;

pub trait Prop {
  type EpollHandler: Handler<EpollEvent, EpollCmd>;

  fn get_epoll_config(&self) -> EpollConfig;

  // TODO deprecate when daemon is implemented
  fn stop(&self);

  fn setup(&mut self, mask: SigSet) -> Result<Epoll<Self::EpollHandler>>;
}

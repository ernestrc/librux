use epoll::{EpollEvent, EpollCmd, Epoll};
use error::Result;
use handler::Handler;

pub mod server;
pub mod signals;

use self::signals::SigSet;

pub trait Prop {
  type EpollHandler: Handler<EpollEvent, EpollCmd>;

  fn setup(&mut self, mask: SigSet) -> Result<Epoll<Self::EpollHandler>>;
}

pub trait Reload {
  fn reload(&mut self);
}

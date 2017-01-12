

use epoll::EpollCmd;
use handler::Handler;
pub use nix::sys::signal::{Signal, SigSet};

pub struct DefaultSigHandler;

impl Handler<Signal, EpollCmd> for DefaultSigHandler {
  fn on_next(&mut self, _: Signal) -> EpollCmd {
    EpollCmd::Shutdown
  }
}

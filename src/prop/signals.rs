use handler::Handler;
pub use nix::sys::signal::{Signal, SigSet};

use poll::EpollCmd;

pub struct DefaultSigHandler;

impl Handler<Signal, EpollCmd> for DefaultSigHandler {
  fn on_next(&mut self, _: Signal) -> EpollCmd {
    EpollCmd::Shutdown
  }
}

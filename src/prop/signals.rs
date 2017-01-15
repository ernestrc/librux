use handler::Handler;
use daemon::DaemonCmd;

pub use nix::sys::signal::{Signal, SigSet};

pub struct DefaultSigHandler;

impl Handler<Signal, DaemonCmd> for DefaultSigHandler {
  fn on_next(&mut self, sig: Signal) -> DaemonCmd {
    match sig {
      Signal::SIGHUP => DaemonCmd::Reload,
      Signal::SIGTERM => DaemonCmd::Shutdown,
      s => panic!("unhandled signal received: {:?}", s),
    }
  }
}

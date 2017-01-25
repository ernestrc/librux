use handler::Handler;
use daemon::DaemonCmd;

pub use nix::sys::signal::{Signal, SigSet};

pub struct DefaultSigHandler {
  next: Option<DaemonCmd>
}

impl DefaultSigHandler {
  pub fn new() -> DefaultSigHandler {
    DefaultSigHandler {
      next: None
    }
  }
}

impl Handler<Signal, DaemonCmd> for DefaultSigHandler {

  fn next(&mut self) -> DaemonCmd {
    match self.next.take() {
      Some(cmd) => cmd,
      _ => DaemonCmd::Continue
    }
  }

  fn on_next(&mut self, sig: Signal) {
    match sig {
      Signal::SIGHUP => self.next = Some(DaemonCmd::Reload),
      Signal::SIGTERM => self.next = Some(DaemonCmd::Shutdown),
      s => panic!("unhandled signal received: {:?}", s),
    }
  }
}

use error::Result;
use handler::*;
use daemon::{Daemon, DaemonCmd};

use nix::sys::signal::{Signal, SigSet};

use prop::Prop;
use prop::signals::DefaultSigHandler;

pub struct DaemonBuilder<S, P> {
  prop: P,
  sig_h: S,
  // TODO sched_policy: c_int,
  sig_mask: SigSet,
}

impl<S, P> DaemonBuilder<S, P>
  where S: Handler<Signal, DaemonCmd> + 'static,
        P: Prop + Send + 'static,
{
  pub fn with_sig_handler(self, handler: S) -> DaemonBuilder<S, P> {
    DaemonBuilder { sig_h: handler, ..self }
  }

  pub fn with_sig_mask(self, mask: SigSet) -> DaemonBuilder<S, P> {
    DaemonBuilder { sig_mask: mask, ..self }
  }

  pub fn run(self) -> Result<()> {
    Daemon::run(self.prop, self.sig_h, self.sig_mask)
  }
}

impl <P> Daemon<DefaultSigHandler, P> 
  where P: Prop {
  pub fn build(prop: P) -> DaemonBuilder<DefaultSigHandler, P> {
    let mut mask = SigSet::empty();
    mask.add(Signal::SIGTERM);
    mask.add(Signal::SIGHUP);

    DaemonBuilder {
      sig_mask: mask,
      sig_h: DefaultSigHandler,
      prop: prop,
    }
  }
}

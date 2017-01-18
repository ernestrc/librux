use daemon::{Daemon, DaemonCmd};
use error::Result;
use handler::Handler;
use libc_sys::{c_int, sched_param, sched_get_priority_max};
use nix::Errno;
use nix::sys::signal::{Signal, SigSet};
use prop::{Prop, Reload};
use prop::signals::DefaultSigHandler;

#[allow(non_camel_case_types)]
pub type sched_policy = c_int;

pub struct DaemonBuilder<S, P> {
  prop: P,
  sig_h: S,
  sig_mask: SigSet,
  sched_opt: Option<(sched_policy, sched_param)>,
}

impl<S, P> DaemonBuilder<S, P>
  where S: Handler<Signal, DaemonCmd> + 'static,
        P: Prop + Reload + Send + 'static,
{
  pub fn with_sig_handler(self, handler: S) -> DaemonBuilder<S, P> {
    DaemonBuilder { sig_h: handler, ..self }
  }

  pub fn with_sig_mask(self, mask: SigSet) -> DaemonBuilder<S, P> {
    DaemonBuilder { sig_mask: mask, ..self }
  }

  pub fn with_sched(self, policy: sched_policy, priority: Option<c_int>) -> DaemonBuilder<S, P> {
    let sched_param_i = sched_param { sched_priority: priority.unwrap_or_else(|| unsafe {
        Errno::result(sched_get_priority_max(policy)).unwrap() })
    };

    DaemonBuilder { sched_opt: Some((policy, sched_param_i)), ..self }
  }

  pub fn run(self) -> Result<()> {
    Daemon::run(self.prop, self.sig_h, self.sig_mask, self.sched_opt)
  }
}

impl<P> Daemon<DefaultSigHandler, P>
  where P: Prop,
{
  pub fn build(prop: P) -> DaemonBuilder<DefaultSigHandler, P> {
    let mut mask = SigSet::empty();
    mask.add(Signal::SIGTERM);
    mask.add(Signal::SIGHUP);

    DaemonBuilder {
      sig_mask: mask,
      sig_h: DefaultSigHandler,
      prop: prop,
      sched_opt: None,
    }
  }
}

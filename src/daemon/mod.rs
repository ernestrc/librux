use epoll::*;
use error::Result;
use handler::*;
use libc_sys::{sched_param, sched_setscheduler, rlimit, prlimit, RLIMIT_RTPRIO};
pub use nix::sys::signal::{Signal, SigSet};
use nix::sys::signalfd::{SignalFd, SFD_NONBLOCK};
use nix::{unistd, Errno};
use prop::*;
use std::os::unix::io::AsRawFd;

mod builder;

pub use self::builder::sched_policy;
pub use libc_sys::{SCHED_FIFO, SCHED_RR, SCHED_OTHER};

/// WIP: New-style daemon
/// http://man7.org/linux/man-pages/man7/daemon.7.html
///
/// Missing:
/// - Force exposing daemon interface via D-Bus along with a D-Bus service activation
///   configuration file
/// - Force Prop to optionally be socket activatable
/// - Optionally notify init system via http://man7.org/linux/man-pages/man3/sd_notify.3.html
pub struct Daemon<S, P> {
  sigfd: SignalFd,
  sig_h: S,
  prop: P,
  terminating: bool
}

impl<S, P> Daemon<S, P>
  where S: Handler<Signal, DaemonCmd> + 'static,
        P: Prop + Reload + Send + 'static,
{
  pub fn run(mut prop: P, sig_h: S, sig_mask: SigSet, sched_opt: Option<(sched_policy, sched_param)>) -> Result<()> {

    sched_opt.map(|(sched_policy, sched_param_i)| {
      // set sched policy
      unsafe {
        let mut rlim = rlimit {
          rlim_cur: sched_param_i.sched_priority as u64,
          rlim_max: sched_param_i.sched_priority as u64,
        };
        Errno::result(prlimit(0, RLIMIT_RTPRIO, &rlim, &mut rlim)).unwrap();
        Errno::result(sched_setscheduler(0, sched_policy, &sched_param_i as *const sched_param))
          .unwrap();
      };
    });

    // add the set of signals to the signal mask
    sig_mask.thread_block()?;

    let sigfd = SignalFd::with_flags(&sig_mask, SFD_NONBLOCK)?;
    let fd = sigfd.as_raw_fd();

    let mut main = prop.setup(sig_mask).unwrap();

    ::std::thread::spawn(move || {
      sig_mask.thread_block().unwrap();
      info!("{:?} starting main event loop", unistd::getpid());
      // run prop's I/O event loop(s)
      main.run();
    });

    let mut aux = Epoll::new_with(Default::default(), |_| {
      Daemon {
        sigfd: sigfd,
        sig_h: sig_h,
        prop: prop,
        terminating: false,
      }
    })?;

    let siginfo = EpollEvent {
      events: EPOLLIN | EPOLLET,
      data: fd as u64,
    };

    // register signalfd with epfd
    aux.epfd.register(fd, &siginfo)?;

    // run aux event loop
    info!("{:?} starting aux event loop", unistd::getpid());
    aux.run();

    Ok(())
  }
}

impl<S, P> Drop for Daemon<S, P> {
  fn drop(&mut self) {
    // signalfd is closed by the SignalFd struct
    // and epfd is closed by Epoll
  }
}

pub enum DaemonCmd {
  Shutdown,
  Reload,
  Continue,
}

impl<S, P> Handler<EpollEvent, EpollCmd> for Daemon<S, P>
  where S: Handler<Signal, DaemonCmd>,
        P: Prop + Reload,
{

  fn next(&mut self) -> EpollCmd {
    if self.terminating {
      return EpollCmd::Shutdown;
    }

    EpollCmd::Poll
  }

  fn on_next(&mut self, ev: EpollEvent) {
    if ev.data == self.sigfd.as_raw_fd() as u64 {
      match self.sigfd.read_signal() {
        Ok(Some(sig)) => {
          self.sig_h.on_next(Signal::from_c_int(sig.ssi_signo as i32).unwrap());
          match self.sig_h.next() {
            DaemonCmd::Reload => self.prop.reload(),
            DaemonCmd::Shutdown => {
              warn!("received signal {:?}. Shutting down ..", sig.ssi_signo);
              self.terminating = true;
            },
            DaemonCmd::Continue => {}
          }
        }
        Ok(None) => debug!("read_signal(): not ready"),
        Err(err) => error!("read_signal(): {}", err),
      };
    }
  }
}

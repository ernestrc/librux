use epoll::*;
use error::Result;
use handler::*;

pub use nix::sys::signal::{Signal, SigSet};

use nix::sys::signalfd::{SignalFd, SFD_NONBLOCK};
use nix::unistd;
use prop::Prop;
use std::os::unix::io::AsRawFd;

mod builder;

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
}

impl<S, P> Daemon<S, P>
  where S: Handler<Signal, DaemonCmd> + 'static,
        P: Prop + Send + 'static
        
{
  pub fn run(mut prop: P, sig_h: S, sig_mask: SigSet) -> Result<()> {

    // add the set of signals to the signal mask
    try!(sig_mask.thread_block());

    let sigfd = try!(SignalFd::with_flags(&sig_mask, SFD_NONBLOCK));
    let fd = sigfd.as_raw_fd();

    let mut main = prop.setup(sig_mask).unwrap();

    ::std::thread::spawn(move || {
      sig_mask.thread_block().unwrap();
      info!("{:?} starting main event loop", unistd::getpid());
      // run prop's I/O event loop(s)
      main.run();
    });

    let mut aux = try!(Epoll::new_with(Default::default(), |_| {
      Daemon {
        sigfd: sigfd,
        sig_h: sig_h,
        prop: prop
      }
    }));

    let siginfo = EpollEvent {
      events: EPOLLIN | EPOLLET,
      data: fd as u64,
    };

    // register signalfd with epfd
    try!(aux.epfd.register(fd, &siginfo));

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
  Reload
}

impl<S, P> Handler<EpollEvent, EpollCmd> for Daemon<S, P>
  where S: Handler<Signal, DaemonCmd>,
        P: Prop
{
  fn on_next(&mut self, ev: EpollEvent) -> EpollCmd {
    if ev.data == self.sigfd.as_raw_fd() as u64 {
      match self.sigfd.read_signal() {
        Ok(Some(sig)) => {
          match self.sig_h.on_next(Signal::from_c_int(sig.ssi_signo as i32).unwrap()) {
            DaemonCmd::Reload => self.prop.reload(),
            DaemonCmd::Shutdown => {
              warn!("received signal {:?}. Shutting down ..", sig.ssi_signo);
              return EpollCmd::Shutdown;
            }
          }
        }
        Ok(None) => debug!("read_signal(): not ready"),
        Err(err) => error!("read_signal(): {}", err),
      }
    }

    EpollCmd::Poll
  }
}

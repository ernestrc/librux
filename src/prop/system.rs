use std::os::unix::io::AsRawFd;

use nix::sys::signalfd::{SignalFd, SFD_NONBLOCK};
use nix::unistd;

pub use nix::sys::signal::{Signal, SigSet};
use nix::sys::signal;

use error::Result;
use handler::*;
use poll::*;
use protocol::*;
use prop::Prop;
use prop::signals::DefaultSigHandler;

pub struct SystemBuilder<S, P> {
    prop: P,
    sig_h: S,
    sig_mask: SigSet,
}

impl<S, P> SystemBuilder<S, P>
    where S: Handler<In = Signal, Out = EpollCmd>,
          P: Prop
{
    pub fn with_sig_handler(self, handler: S) -> SystemBuilder<S, P> {
        SystemBuilder { sig_h: handler, ..self }
    }

    pub fn with_sig_mask(self, mask: SigSet) -> SystemBuilder<S, P> {
        SystemBuilder { sig_mask: mask, ..self }
    }

    pub fn start<'p, O>(self, protocol: &'p O) -> Result<()>
        where O: StaticProtocol<'p, EpollEvent, EpollCmd>
    {
        System::start(self.prop, self.sig_h, self.sig_mask, protocol)
    }
}

pub struct System<S, P> {
    parentpid: i32,
    sigfd: SignalFd,
    sig_h: S,
    prop: P,
}

impl<P> System<DefaultSigHandler, P>
    where P: Prop,
{
    pub fn build(prop: P) -> SystemBuilder<DefaultSigHandler, P> {
        // default mask
        let mut mask = SigSet::empty();
        mask.add(Signal::SIGINT);
        mask.add(Signal::SIGTERM);
        SystemBuilder {
            sig_mask: mask,
            sig_h: DefaultSigHandler,
            prop: prop,
        }
    }
}

impl<P, S> System<S, P>
    where P: Prop,
          S: Handler<In = Signal, Out = EpollCmd>
{

    pub fn start<'p, O>(mut prop: P, sig_h: S, mut sig_mask: SigSet, protocol: &'p O) -> Result<()>
        where O: StaticProtocol<'p, EpollEvent, EpollCmd>
    {

        sig_mask.add(Signal::SIGCHLD);

        let econfig = prop.get_epoll_config();

        // add the set of signals to the signal mask
        // of the main thread
        try!(sig_mask.thread_block());

        let sigfd = try!(SignalFd::with_flags(&sig_mask, SFD_NONBLOCK));
        let fd = sigfd.as_raw_fd();

        // TODO provide rux own logging backend
        // let log = lb.setup(&epfd).unwrap();
        // ::log::set_logger(|max_log_level| {
        //         max_log_level.set(lb.level().to_log_level_filter());
        //         log
        //     }).unwrap();

        let parentpid = unistd::getpid();

        let mut main: Epoll<O::H> = prop.setup(sig_mask, protocol).unwrap();

        match unistd::fork() {
            Ok(unistd::ForkResult::Parent { child, .. }) => {
                sig_mask.thread_block().unwrap();
                debug!("{:?} created I/O child process n {} with pid {}",
                       unistd::getpid(),
                       0,
                       child);
                info!("{:?} starting main event loop", unistd::getpid());
                // run impl's I/O event loop(s)
                main.run();
                return Err("terminated I/O child process".into());
            }
            Ok(unistd::ForkResult::Child) => {
                // continue
            }
            Err(e) => {
                return Err(e.into());
            }
        };

        let mut aux = try!(Epoll::new_with(econfig, |_| {
            System {
                sigfd: sigfd,
                sig_h: sig_h,
                parentpid: parentpid,
                prop: prop,
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

impl<L, I> Drop for System<L, I> {
    fn drop(&mut self) {
        // signalfd is closed by the SignalFd struct
        // and epfd is closed by EpollFd
    }
}

impl<S, R> Handler for System<S, R>
    where S: Handler<In = Signal, Out = EpollCmd>,
          R: Prop
{
    type In = EpollEvent;
    type Out = EpollCmd;

    fn ready(&mut self, ev: EpollEvent) -> EpollCmd {
        trace!("ready(): {:?}: {:?}", ev.data, ev.events);
        if ev.data == self.sigfd.as_raw_fd() as u64 {
            match self.sigfd.read_signal() {
                Ok(Some(sig)) if Signal::from_c_int(sig.ssi_signo as i32).unwrap() ==
                                 Signal::SIGCHLD => {
                    error!("child process quit unexpectedly ({:?}). Shutting down..",
                           sig.ssi_signo);
                    self.prop.stop();
                    signal::kill(self.parentpid, signal::Signal::SIGKILL).unwrap();
                    return EpollCmd::Shutdown;
                }
                Ok(Some(sig)) => {
                    match self.sig_h.ready(Signal::from_c_int(sig.ssi_signo as i32).unwrap()) {
                        EpollCmd::Shutdown => {
                            warn!("{:?} received signal {:?}. Shutting down parent pid {:?}..",
                                  unistd::getpid(),
                                  sig.ssi_signo,
                                  self.parentpid);
                            // terminate child processes
                            self.prop.stop();
                            signal::kill(self.parentpid, signal::Signal::SIGKILL).unwrap();
                            // terminate server aux loop
                            return EpollCmd::Shutdown;
                        }
                        _ => {}
                    }
                }
                Ok(None) => debug!("read_signal(): not ready"),
                Err(err) => error!("read_signal(): {}", err),
            }
        }

        EpollCmd::Poll
    }
}

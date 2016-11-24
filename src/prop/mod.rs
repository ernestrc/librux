use std::os::unix::io::AsRawFd;

use nix::sys::signalfd::*;
use nix::sys::signal::{SIGINT, SIGTERM, SIGCHLD, Signal};
use nix::unistd;

use error::Result;
use handler::*;
use poll::*;
use logging::LoggingBackend;
use protocol::*;

pub mod server;

pub struct Prop<L, R> {
    sigfd: SignalFd,
    lb: L,
    parentpid: i32,
    run: R,
}

impl<'p, L, R> Prop<L, R>
    where L: LoggingBackend,
          R: Run
{
    pub fn create<P>(mut run: R, lb: L, p: &'p P) -> Result<()>
        where P: StaticProtocol<'p, EpollEvent, ()>
    {

        trace!("bind()");

        // signal mask to share across threads
        let mut mask = SigSet::empty();
        mask.add(SIGINT);
        mask.add(SIGTERM);
        mask.add(SIGCHLD);

        let econfig = run.get_epoll_config();

        // add the set of signals to the signal mask
        // of the main thread
        try!(mask.thread_block());

        let sigfd = try!(SignalFd::with_flags(&mask, SFD_NONBLOCK));
        let fd = sigfd.as_raw_fd();

        // TODO setup logging
        // delegate logging registering to logging backend
        // TODO let log = lb.setup(&epfd).unwrap();

        // ::log::set_logger(|max_log_level| {
        //         max_log_level.set(lb.level().to_log_level_filter());
        //         log
        //     }).unwrap();


        let parentpid = unistd::getpid();

        let mut main = run.setup(mask, p).unwrap();

        match unistd::fork() {
            Ok(unistd::ForkResult::Parent { child, .. }) => {
                mask.thread_block().unwrap();
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

        let mut aux = try!(Epoll::new_with(econfig, |epfd| {
            Prop {
                sigfd: sigfd,
                lb: lb,
                parentpid: parentpid,
                run: run,
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

impl<L, I> Drop for Prop<L, I> {
    fn drop(&mut self) {
        // signalfd is closed by the SignalFd struct
        // and epfd is closed by EpollFd
    }
}

impl<L, R> Handler for Prop<L, R>
    where L: LoggingBackend,
          R: Run
{
    type In = EpollEvent;
    type Out = ();

    fn reset(&mut self) {
        self.lb.reset();
    }

    // TODO should take sig handler
    fn ready(&mut self, ev: EpollEvent) -> Option<()> {
        trace!("ready(): {:?}: {:?}", ev.data, ev.events);
        if ev.data == self.sigfd.as_raw_fd() as u64 {
            match self.sigfd.read_signal() {
                Ok(Some(sig)) if Signal::from_c_int(sig.ssi_signo as i32).unwrap() == SIGCHLD => {
                    error!("child process quit unexpectedly ({:?}). Shutting down..",
                           sig.ssi_signo);
                    Some(())
                }
                Ok(Some(sig)) => {
                    warn!("{:?} received signal {:?}. Shutting down parent pid {:?}..",
                          unistd::getpid(),
                          sig.ssi_signo,
                          self.parentpid);
                    // terminate child processes
                    self.run.stop();
                    signal::kill(self.parentpid, signal::Signal::SIGKILL).unwrap();
                    // terminate server aux loop
                    Some(())
                }
                Ok(None) => {
                    debug!("read_signal(): not ready");
                    None
                }
                Err(err) => {
                    error!("read_signal(): {}", err);
                    None
                }
            }
        } else {
            // delegate events to logging backend
            self.lb.ready(ev)
        }
    }
}


pub trait Run {
    fn get_epoll_config(&self) -> EpollConfig;

    fn stop(&self);

    fn setup<'p, P: StaticProtocol<'p, EpollEvent, ()>>(&mut self,
                                                        mask: SigSet,
                                                        protocol: &'p P)
                                                        -> Result<Epoll<P::H>>;
}

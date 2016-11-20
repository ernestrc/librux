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

pub struct Prop<'p, L: LoggingBackend, P: StaticProtocol<'p, EpollEvent, ()>, I: Run<'p, P>> {
    sigfd: SignalFd,
    lb: L,
    parentpid: i32,
    im: I,
    _marker: ::std::marker::PhantomData<&'p bool>,
    _marker2: ::std::marker::PhantomData<P>,
}

impl<'p, L, P, I> Prop<'p, L, P, I>
    where L: LoggingBackend,
          P: StaticProtocol<'p, EpollEvent, ()>,
          I: Run<'p, P>
{
    pub fn create(mut im: I, lb: L, p: &'p P) -> Result<()> {

        trace!("bind()");

        // signal mask to share across threads
        let mut mask = SigSet::empty();
        mask.add(SIGINT);
        mask.add(SIGTERM);
        mask.add(SIGCHLD);

        let econfig = im.get_epoll_config();

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

        let mut main = im.setup(mask, p).unwrap();

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
                im: im,
                _marker: ::std::marker::PhantomData {},
                _marker2: ::std::marker::PhantomData {},
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

impl<'p, L, P, I> Drop for Prop<'p, L, P, I>
    where L: LoggingBackend,
          P: StaticProtocol<'p, EpollEvent, ()>,
          I: Run<'p, P>
{
    fn drop(&mut self) {
        // signalfd is closed by the SignalFd struct
        // and epfd is closed by EpollFd
    }
}

impl<'p, L, P, I> Handler for Prop<'p, L, P, I>
    where L: LoggingBackend,
          P: StaticProtocol<'p, EpollEvent, ()>,
          I: Run<'p, P>
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
                    self.im.stop();
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


pub trait Run<'p, P: StaticProtocol<'p, EpollEvent, ()>> {
    fn get_epoll_config(&self) -> EpollConfig;

    fn stop(&self);

    fn setup(&mut self, mask: SigSet, protocol: &'p P) -> Result<Epoll<P::H>>;
}

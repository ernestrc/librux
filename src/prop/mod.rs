use std::os::unix::io::AsRawFd;

use nix::sys::signalfd::*;
use nix::sys::signal::{SIGINT, SIGTERM};
use nix::unistd;

use error::Result;
use handler::*;
use poll::*;
use logging::LoggingBackend;
use protocol::*;

pub mod server;

pub struct Prop<L: LoggingBackend> {
    sigfd: SignalFd,
    lb: L,
    childpid: i32,
}

impl<L> Prop<L>
    where L: LoggingBackend
{
    pub fn create<'p, P, I>(im: I, lb: L, p: &'p P) -> Result<()>
        where P: StaticProtocol<'p, EpollEvent, ()>,
              I: Run<'p, P>
    {

        trace!("bind()");

        // signal mask to share across threads
        let mut mask = SigSet::empty();
        mask.add(SIGINT);
        mask.add(SIGTERM);

        let econfig = im.get_epoll_config();

        // add the set of signals to the signal mask
        // of the main thread
        try!(mask.thread_block());

        let sigfd = try!(SignalFd::with_flags(&mask, SFD_NONBLOCK));
        let fd = sigfd.as_raw_fd();

        let childpid = match unistd::fork() {
            Ok(unistd::ForkResult::Parent { child, .. }) => {
                debug!("created I/O child process n {} with pid {}", 0, child);
                child
            }
            Ok(unistd::ForkResult::Child) => {
                mask.thread_block().unwrap();
                // run impl's I/O event loop(s)
                im.run(mask, p).unwrap();
                return Err("terminated I/O child process".into());
            }
            Err(e) => {
                return Err(e.into());
            }
        };

        let mut epoll = try!(Epoll::new_with(econfig, |epfd| {

            // delegate logging registering to logging backend
            let log = lb.setup(&epfd).unwrap();

            ::log::set_logger(|max_log_level| {
                    max_log_level.set(lb.level().to_log_level_filter());
                    log
                })
                .unwrap();

            Prop {
                sigfd: sigfd,
                lb: lb,
                childpid: childpid
            }
        }));

        let siginfo = EpollEvent {
            events: EPOLLIN | EPOLLET,
            data: fd as u64,
        };

        // register signalfd with epfd
        try!(epoll.epfd.register(fd, &siginfo));

        // run aux event loop
        epoll.run();

        Ok(())
    }
}

impl<L: LoggingBackend> Drop for Prop<L> {
    fn drop(&mut self) {
        // signalfd is closed by the SignalFd struct
        // and epfd is closed by EpollFd
    }
}

impl<L: LoggingBackend> Handler for Prop<L> {
    type In = EpollEvent;
    type Out = ();

    fn reset(&mut self) {
        self.lb.reset();
    }

    fn ready(&mut self, ev: EpollEvent) -> Option<()> {
        trace!("ready(): {:?}: {:?}", ev.data, ev.events);
        if ev.data == self.sigfd.as_raw_fd() as u64 {
            match self.sigfd.read_signal() {
                // TODO should take sig handler
                Ok(Some(sig)) => {
                    // stop server's event loop, as the signal mask
                    // contains SIGINT and SIGTERM
                    warn!("received signal {:?}. Shutting down..", sig.ssi_signo);
                    // terminate impl child process
                    signal::kill(self.childpid, signal::Signal::SIGKILL).unwrap();
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

    fn run(self, mask: SigSet, proto: &'p P) -> Result<()>;
}

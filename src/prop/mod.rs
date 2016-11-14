use std::thread;
use std::os::unix::io::AsRawFd;

use nix::sys::signalfd::*;
use nix::sys::signal::{SIGINT, SIGTERM};

use error::Result;
use handler::*;
use poll::*;
use super::logging::LoggingBackend;

pub mod mux;

/// Takes care of signals and logging, and delegates
/// the rest to Server implementations.
pub struct Prop<L: LoggingBackend> {
    sigfd: SignalFd,
    lb: L,
    terminated: bool,
}

// TODO should take SignalHandler
// TODO provide config to tune sched_setaffinity
unsafe impl<L: LoggingBackend + Send> Send for Prop<L> {}

impl<L> Prop<L>
    where L: LoggingBackend + Send
{
    /// Instantiates new Prop with the given implementation
    /// and logging backend
    pub fn create<I: Server + Send + 'static>(im: I, lb: L) -> Result<()> {

        trace!("bind()");

        // signal mask to share across threads
        let mut mask = SigSet::empty();
        mask.add(SIGINT);
        mask.add(SIGTERM);

        let econfig = im.get_epoll_config();

        thread::spawn(move || {
            try!(mask.thread_block());
            // run impl's I/O event loop(s)
            im.bind(mask)
        });

        // add the set of signals to the signal mask
        // of the main thread
        try!(mask.thread_block());

        let sigfd = try!(SignalFd::with_flags(&mask, SFD_NONBLOCK));
        let fd = sigfd.as_raw_fd();

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
                terminated: false,
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

impl<L: LoggingBackend> Handler<EpollEvent> for Prop<L> {
    fn is_terminated(&self) -> bool {
        self.terminated
    }

    fn ready(&mut self, ev: &EpollEvent) {
        trace!("ready(): {:?}: {:?}", ev.data, ev.events);
        if ev.data == self.sigfd.as_raw_fd() as u64 {
            match self.sigfd.read_signal() {
                Ok(Some(sig)) => {
                    // stop server's event loop, as the signal mask
                    // contains SIGINT and SIGTERM
                    warn!("received signal {:?}. Shutting down..", sig.ssi_signo);
                    // terminate server aux loop
                    self.terminated = true;
                }
                Ok(None) => debug!("read_signal(): not ready to read"),
                Err(err) => error!("read_signal(): {}", err),
            }
        } else {
            // delegate events to logging backend
            self.lb.ready(ev)
        }
    }
}


pub trait Server {
    fn stop(&mut self);

    fn get_epoll_config(&self) -> EpollConfig;

    fn bind(self, mask: SigSet) -> Result<()>;
}

use std::thread;
use std::os::unix::io::AsRawFd;
use std::thread::JoinHandle;

use nix::sys::signalfd::*;
use nix::sys::signal::{SIGINT, SIGTERM};

use error::Result;
use handler::*;
use poll::*;
use super::logging::LoggingBackend;

pub mod simplemux;

/// Server facade.
/// TODO rename to something more generic, as it could perfectly be used
/// to implement clients
///
/// Takes care of signals and logging, and delegates bind/listen/accept
/// logic to `ServerImpl`.
pub struct Server<L: LoggingBackend> {
    epfd: EpollFd,
    sigfd: SignalFd,
    // to speed up `ready()`
    _sigfd: u64,
    lb: L,
    sproc: JoinHandle<Result<()>>,
    terminated: bool,
}

unsafe impl<L: LoggingBackend + Send> Send for Server<L> {}

impl<L> Server<L>
    where L: LoggingBackend + Send
{
    /// Instantiates new Server with the given implementation
    /// and logging backend
    pub fn bind<I: ServerImpl + Send + 'static>(im: I, lb: L) -> Result<()> {

        trace!("bind()");

        // signal mask to share across threads
        let mut mask = SigSet::empty();
        mask.add(SIGINT).unwrap();
        mask.add(SIGTERM).unwrap();

        let loop_ms = im.get_loop_ms();

        let sproc = thread::spawn(move || {
            try!(mask.thread_block());
            // run impl's I/O event loop(s)
            im.bind(mask)
        });

        // add the set of signals to the signal mask
        // of the main thread
        try!(mask.thread_block());

        let sigfd = try!(SignalFd::with_flags(&mask, SFD_NONBLOCK));
        let fd = sigfd.as_raw_fd();

        let mut epoll = try!(Epoll::new_with(loop_ms, |epfd| {

            // delegate logging registering to logging backend
            let log = lb.setup(&epfd).unwrap();

            ::log::set_logger(|max_log_level| {
                    max_log_level.set(lb.level().to_log_level_filter());
                    log
                })
                .unwrap();

            Box::new(Server {
                epfd: epfd,
                sigfd: sigfd,
                _sigfd: fd as u64,
                lb: lb,
                sproc: sproc,
                terminated: false,
            })
        }));

        let siginfo = EpollEvent {
            events: EPOLLIN | EPOLLOUT | EPOLLHUP | EPOLLRDHUP | EPOLLERR,
            data: fd as u64,
        };

        // register signalfd with epfd
        try!(epoll.epfd.register(fd, &siginfo));

        // run aux event loop
        epoll.run()
    }
}

impl<L: LoggingBackend> Drop for Server<L> {
    fn drop(&mut self) {
        // signalfd is closed by the SignalFd struct
        // and epfd is closed by EpollFd
        // so nothing to do here
    }
}

impl<L: LoggingBackend> Handler for Server<L> {

    fn is_terminated(&self) -> bool {
        self.terminated
    }

    fn ready(&mut self, ev: &EpollEvent) {
        trace!("ready(): {:?}: {:?}", ev.data, ev.events);
        if ev.data == self._sigfd {
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


pub trait ServerImpl {

    fn stop(&mut self);

    fn get_loop_ms(&self) -> isize;

    fn bind(self, mask: SigSet) -> Result<()>;
}

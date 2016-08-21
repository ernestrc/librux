use std::os::unix::io::RawFd;
use std::cell::RefCell;
use std::boxed::Box;

use nix::unistd;
use slab::Slab;

use error::{Result, Error};
use handler::*;
use poll::*;

// TODO should take whether epoll mode is ET or LT
pub struct SyncHandler<F: EpollProtocol> {
    epfd: EpollFd,
    handlers: Slab<RefCell<Box<Handler>>, usize>,
    eproto: F,
}

impl<F: EpollProtocol> SyncHandler<F> {
    pub fn new(epfd: EpollFd, eproto: F, max_handlers: usize) -> SyncHandler<F> {
        trace!("new()");
        SyncHandler {
            epfd: epfd,
            handlers: Slab::new(max_handlers),
            eproto: eproto,
        }
    }

    fn notify(&mut self, fd: RawFd, id: usize, event: &EpollEvent) {
        trace!("notify()");

        let events = event.events;

        if events.contains(EPOLLRDHUP) || events.contains(EPOLLHUP) {

            trace!("socket's fd {}: EPOLLHUP", fd);
            match self.handlers.remove(id) {
                None => error!("on_close(): handler not found"),
                Some(_) => {}
            }
            perror!("unregister()", self.epfd.unregister(fd));
            perror!("close()", unistd::close(fd));
            debug!("handlers: {:?}", self.handlers);
        } else {
            let handler = &mut self.handlers[id];
            handler.borrow_mut().ready(event);
        }
    }
}

impl<F: EpollProtocol> Handler for SyncHandler<F> {
    fn ready(&mut self, ev: &EpollEvent) {
        trace!("ready()");

        match self.eproto.decode(ev.data) {

            Action::New(proto, fd) => {
                // TODO handle too many handlers
                if let Ok(id) = self.handlers
                    .insert(RefCell::new(self.eproto.new(proto, fd, self.epfd)))
                    .map_err(|_| "reached maximum number of handlers") {

                    let action: Action<F> = Action::Notify(id, fd);

                    let interest = EpollEvent {
                        events: EPOLLIN | EPOLLOUT | EPOLLHUP | EPOLLRDHUP,
                        data: self.eproto.encode(action),
                    };

                    match self.epfd.reregister(fd, &interest) {
                        Ok(_) => self.notify(fd, id, ev),
                        Err(e) => report_err!("reregister()", e),
                    }
                }
            }

            Action::Notify(id, fd) => self.notify(fd, id, ev),
        }
    }
}

pub type HandlerId = usize;

pub enum Action<P: EpollProtocol> {
    New(P::Protocol, RawFd),
    Notify(HandlerId, RawFd),
}

pub trait EpollProtocol
    where Self: Sized + Send + Copy
{
    type Protocol: From<usize> + Into<usize>;

    fn new(&self, p: Self::Protocol, fd: RawFd, epfd: EpollFd) -> Box<Handler>;

    fn encode(&self, action: Action<Self>) -> u64 {
        match action {
            Action::Notify(id, fd) => ((fd as u64) << 31) | ((id as u64) << 15) | 0,
            Action::New(protocol, fd) => {
                let protocol: usize = protocol.into();
                ((fd as u64) << 31) | ((protocol as u64) << 15) | 1
            }
        }
    }

    fn decode(&self, data: u64) -> Action<Self> {
        let arg1 = ((data >> 15) & 0xffff) as usize;
        let fd = (data >> 31) as i32;
        match data & 0x7fff {
            0 => Action::Notify(arg1, fd),
            1 => Action::New(From::from(arg1), fd),
            a => panic!("unrecognized action: {}", a),
        }
    }
}

#[cfg(test)]
mod tests {
    use error::Result;
    use poll::*;
    use handler::Handler;
    use RawFd;
    use super::*;

    #[derive(Clone, Copy)]
    struct TestEpollProtocol;

    const PROTO1: usize = 1;

    struct TestHandler {
        on_close: bool,
        on_error: bool,
        on_readable: bool,
        on_writable: bool,
    }

    impl Handler for TestHandler {
        fn on_error(&mut self) -> Result<()> {
            self.on_error = true;
            Ok(())
        }

        fn on_close(&mut self) -> Result<()> {
            self.on_close = true;
            Ok(())
        }

        fn on_readable(&mut self) -> Result<()> {
            self.on_readable = true;
            Ok(())
        }

        fn on_writable(&mut self) -> Result<()> {
            self.on_writable = true;
            Ok(())
        }
    }

    impl EpollProtocol for TestEpollProtocol {
        type Protocol = usize;

        fn new(&self, _: usize, _: RawFd, _: EpollFd) -> Box<Handler> {
            Box::new(TestHandler {
                on_close: false,
                on_error: false,
                on_readable: false,
                on_writable: false,
            })
        }
    }

    #[test]
    fn decode_encode_new_action() {
        let test = TestEpollProtocol;
        let data = test.encode(Action::New(PROTO1, ::std::i32::MAX));

        if let Action::New(protocol, fd) = test.decode(data) {
            assert!(protocol == PROTO1);
            assert!(fd == ::std::i32::MAX);
        } else {
            panic!("action is not Action::New")
        }
    }

    #[test]
    fn decode_encode_notify_action() {
        let test = TestEpollProtocol;
        let data = test.encode(Action::Notify(10110, 0));

        if let Action::Notify(id, fd) = test.decode(data) {
            assert!(id == 10110);
            assert!(fd == 0);
        } else {
            panic!("action is not Action::Notify")
        }
    }
}

use std::os::unix::io::RawFd;
use std::cell::RefCell;
use std::boxed::Box;

use nix::unistd;
use slab::Slab;

use error::Error;
use handler::*;
use protocol::{IOProtocol, Action};
use poll::*;

// TODO should take whether epoll mode is ET or LT (as marker traits)
pub struct SyncHandler<F: IOProtocol> {
    epfd: EpollFd,
    handlers: Slab<RefCell<Box<Handler>>, usize>,
    eproto: F,
}

impl<F: IOProtocol> SyncHandler<F> {
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

impl<F: IOProtocol> Handler for SyncHandler<F> {

    fn is_terminated(&self) -> bool {
        false
    }

    fn ready(&mut self, ev: &EpollEvent) {
        trace!("ready()");

        match self.eproto.decode(ev.data) {

            Action::New(proto, fd) => {
                if let Ok(id) = self.handlers
                    .insert(RefCell::new(self.eproto.get_handler(proto, fd, self.epfd))) {
                    // TODO handle too many handlers
                    // .map_err(|_| "reached maximum number of handlers") {

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

use std::os::unix::io::RawFd;

use nix::unistd::close as rclose;
use nix::sys::socket::*;
use slab::Slab;

use handler::Handler;
use buf::ByteBuffer;
use error::*;
use poll::*;
use protocol::*;

pub struct SyncMux<H: Handler<MuxEvent>, P: StaticProtocol<MuxEvent, H>> {
    epfd: EpollFd,
    buffers: Vec<ByteBuffer>,
    handlers: Slab<H, usize>,
    protocol: P,
    is_terminated: bool,
}

pub struct MuxEvent {
    pub kind: EpollEventKind,
    pub fd: RawFd,
    pub buffer: *mut ByteBuffer,
}

impl MuxEvent {
    #[inline]
    pub fn buffer<'a>(&'a self) -> &'a mut ByteBuffer {
        unsafe { &mut *self.buffer }
    }
}

impl<H: Handler<MuxEvent>, P: StaticProtocol<MuxEvent, H>> SyncMux<H, P> {
    pub fn new(buffer_size: usize,
               max_handlers: usize,
               epfd: EpollFd,
               protocol: P)
               -> SyncMux<H, P> {
        SyncMux {
            epfd: epfd,
            buffers: vec!(ByteBuffer::with_capacity(buffer_size); max_handlers),
            handlers: Slab::with_capacity(max_handlers),
            protocol: protocol,
            is_terminated: false,
        }
    }

    fn new_handler(protocol: &P,
                   proto: P::Protocol,
                   clifd: RawFd,
                   epfd: &EpollFd,
                   handlers: &mut Slab<H, usize>)
                   -> Result<(usize, RawFd)> {
        match handlers.vacant_entry() {
            Some(entry) => {

                let i = entry.index();
                let conn = protocol.get_handler(proto, *epfd, i);
                entry.insert(conn);

                let action = Action::Notify(i, clifd);
                // TODO get_handler should be `next` and should return somehow interests
                let interest = EpollEvent {
                    events: EPOLLIN | EPOLLOUT | EPOLLRDHUP | EPOLLET,
                    data: protocol.encode(action),
                };

                match epfd.register(clifd, &interest) {
                    Ok(_) => {}
                    Err(e) => {
                        error!("closing: {:?}", e);
                        perror!("{}", rclose(clifd));
                        return Err(e);
                    }
                };
                trace!("epoll_ctl: registered interests for {}", clifd);

                Ok((i, clifd))
            }
            None => Err("reached maximum number of handlers".into()),
        }
    }

    fn decode(protocol: &P,
              epfd: &EpollFd,
              event: &EpollEvent,
              handlers: &mut Slab<H, usize>)
              -> Result<Option<(usize, RawFd)>> {

        match protocol.decode(event.data) {
            Action::New(proto, clifd) => {
                let (idx, fd) = Self::new_handler(protocol, proto, clifd, epfd, handlers)?;
                Ok(Some((idx, fd)))
            },
            Action::Notify(i, fd) => Ok(Some((i, fd))),
            Action::NoAction(data) => {
                let srvfd = data as i32;
                match eintr!(accept4, "accept4", srvfd, SOCK_NONBLOCK) {
                    Ok(Some(clifd)) => {
                        trace!("accept4: accepted new tcp client {}", &clifd);
                        Self::new_handler(protocol, From::from(1_usize), clifd, epfd, handlers)?;
                        Ok(None)
                    }
                    Ok(None) => Err("accept4: socket not ready".into()),
                    Err(e) => Err(e.into()),
                }
            }
        }
    }
}

impl<H, P> Handler<EpollEvent> for SyncMux<H, P>
    where H: Handler<MuxEvent>,
          P: StaticProtocol<MuxEvent, H>
{
    fn is_terminated(&self) -> bool {
        self.is_terminated
    }

    fn ready(&mut self, event: &EpollEvent) {

        let (idx, fd) =
            match SyncMux::decode(&self.protocol, &self.epfd, event, &mut self.handlers) {
                Ok(Some((idx, fd))) => (idx, fd),
                Ok(None) => {
                    // new connection
                    return;
                }
                Err(e) => {
                    error!("{}", e);
                    return;
                }
            };

        let buffer = &mut self.buffers[idx];

        self.handlers[idx].ready(&MuxEvent {
            buffer: buffer as *mut ByteBuffer,
            kind: event.events,
            fd: fd,
        });

        if self.handlers[idx].is_terminated() {
            buffer.clear();
            self.handlers.remove(idx);
        }
    }
}

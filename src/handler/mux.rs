use std::os::unix::io::RawFd;

use nix::unistd::close as rclose;
use nix::sys::socket::*;
use slab::Slab;

use handler::Handler;
use buf::ByteBuffer;
use error::*;
use poll::*;
use protocol::*;

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

pub struct SSyncMux<'p, P: StaticProtocol<'p, MuxEvent, ()> + 'p> {
    epfd: EpollFd,
    buffers: Vec<ByteBuffer>,
    handlers: Slab<P::H, usize>,
    protocol: &'p P,
}

impl<'p, P: StaticProtocol<'p, MuxEvent, ()>> SSyncMux<'p, P> {
    pub fn new(buffer_size: usize,
               max_handlers: usize,
               epfd: EpollFd,
               protocol: &'p P)
               -> SSyncMux<'p, P> {
        SSyncMux {
            epfd: epfd,
            buffers: vec!(ByteBuffer::with_capacity(buffer_size); max_handlers),
            handlers: Slab::with_capacity(max_handlers),
            protocol: protocol,
        }
    }

    fn new_handler(protocol: &'p P,
                   proto: Position<P::Protocol>,
                   clifd: RawFd,
                   epfd: &EpollFd,
                   handlers: &mut Slab<P::H, usize>)
                   -> Result<(usize, RawFd)> {
        match handlers.vacant_entry() {
            Some(entry) => {

                let i = entry.index();
                let h = protocol.get_handler(proto, *epfd, i);
                entry.insert(h);

                let action = Action::Notify(i, clifd);
                // TODO interest should be passed by protocol/handler
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

    fn decode(protocol: &'p P,
              epfd: &EpollFd,
              event: EpollEvent,
              handlers: &mut Slab<P::H, usize>)
              -> Result<Option<(usize, RawFd)>> {

        match protocol.decode(event.data) {
            Action::New(proto, clifd) => {
                let (idx, fd) =
                    Self::new_handler(protocol, Position::Handler(proto), clifd, epfd, handlers)?;
                Ok(Some((idx, fd)))
            }
            Action::Notify(i, fd) => Ok(Some((i, fd))),
            Action::NoAction(data) => {
                let srvfd = data as i32;
                match eintr!(accept4, "accept4", srvfd, SockFlag::empty()) {
                    Ok(Some(clifd)) => {
                        trace!("accept4: accepted new tcp client {}", &clifd);
                        Self::new_handler(protocol, Position::Root, clifd, epfd, handlers)?;
                        Ok(None)
                    }
                    Ok(None) => Err("accept4: socket not ready".into()),
                    Err(e) => Err(e.into()),
                }
            }
        }
    }
}

impl<'p, P> Handler for SSyncMux<'p, P>
    where P: StaticProtocol<'p, MuxEvent, ()>
{
    type In = EpollEvent;
    type Out = ();

    fn reset(&mut self) {
        self.handlers.clear()
    }

    fn ready(&mut self, event: EpollEvent) -> Option<()> {

        let (idx, fd) =
            match SSyncMux::decode(self.protocol, &self.epfd, event, &mut self.handlers) {
                Ok(Some((idx, fd))) => (idx, fd),
                Ok(None) => {
                    // new connection
                    return None;
                }
                Err(e) => {
                    error!("{}", e);
                    return None;
                }
            };

        let buffer = &mut self.buffers[idx];

        if let Some(_) = self.handlers[idx].ready(MuxEvent {
            buffer: buffer as *mut ByteBuffer,
            kind: event.events,
            fd: fd,
        }) {
            buffer.clear();
            self.handlers.remove(idx);
        }

        None
    }
}

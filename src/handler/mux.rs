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
    buffer: *mut ByteBuffer,
}

impl MuxEvent {
    #[inline]
    pub fn buffer<'a>(&'a self) -> &'a mut ByteBuffer {
        unsafe { &mut *self.buffer }
    }
}

pub enum MuxCmd {
    Clear,
    Keep,
}

pub struct SyncMux<'p, P: StaticProtocol<'p, MuxEvent, MuxCmd> + 'p> {
    epfd: EpollFd,
    buffers: Vec<ByteBuffer>,
    handlers: Slab<P::H, usize>,
    protocol: &'p P,
    interests: EpollEventKind,
}

impl<'p, P: StaticProtocol<'p, MuxEvent, MuxCmd>> SyncMux<'p, P> {
    pub fn new(buffer_size: usize,
               max_handlers: usize,
               interests: EpollEventKind,
               epfd: EpollFd,
               protocol: &'p P)
               -> SyncMux<'p, P> {
        SyncMux {
            epfd: epfd,
            buffers: vec!(ByteBuffer::with_capacity(buffer_size); max_handlers),
            handlers: Slab::with_capacity(max_handlers),
            protocol: protocol,
            interests: interests,
        }
    }

    #[inline]
    fn new_handler(protocol: &'p P,
                   proto: Position<P::Protocol>,
                   clifd: RawFd,
                   interests: EpollEventKind,
                   epfd: &EpollFd,
                   handlers: &mut Slab<P::H, usize>)
                   -> Result<(usize, RawFd)> {
        match handlers.vacant_entry() {
            Some(entry) => {

                let i = entry.index();
                let h = protocol.get_handler(proto, *epfd, i);
                entry.insert(h);

                let action = Action::Notify(i, clifd);

                let interest = EpollEvent {
                    events: interests,
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

    #[inline]
    fn decode(protocol: &'p P,
              epfd: &EpollFd,
              event: EpollEvent,
              interests: EpollEventKind,
              handlers: &mut Slab<P::H, usize>)
              -> Result<Option<(usize, RawFd)>> {

        match protocol.decode(event.data) {
            Action::New(proto, clifd) => {
                let (idx, fd) =
                    Self::new_handler(protocol, Position::Handler(proto), clifd, interests, epfd, handlers)?;
                Ok(Some((idx, fd)))
            }
            Action::Notify(i, fd) => Ok(Some((i, fd))),
            Action::NoAction(data) => {
                let srvfd = data as i32;
                match eintr!(accept4, "accept4", srvfd, SockFlag::empty()) {
                    Ok(Some(clifd)) => {
                        trace!("accept4: accepted new tcp client {}", &clifd);
                        Self::new_handler(protocol, Position::Root, clifd, interests, epfd, handlers)?;
                        Ok(None)
                    }
                    Ok(None) => Err("accept4: socket not ready".into()),
                    Err(e) => Err(e.into()),
                }
            }
        }
    }
}

impl<'p, P> Handler for SyncMux<'p, P>
    where P: StaticProtocol<'p, MuxEvent, MuxCmd>
{
    type In = EpollEvent;
    type Out = EpollCmd;

    fn ready(&mut self, event: EpollEvent) -> EpollCmd {

        let (idx, fd) =
            match SyncMux::decode(self.protocol, &self.epfd, event, self.interests, &mut self.handlers) {
                Ok(Some((idx, fd))) => (idx, fd),
                Ok(None) => {
                    // new connection
                    return EpollCmd::Poll;
                }
                Err(e) => {
                    error!("{}", e);
                    return EpollCmd::Poll;
                }
            };

        let buffer = &mut self.buffers[idx];

        match self.handlers[idx].ready(MuxEvent {
            buffer: buffer as *mut ByteBuffer,
            kind: event.events,
            fd: fd,
        }) {
            MuxCmd::Clear => {
                buffer.clear();
                self.handlers.remove(idx);
            }
            _ => {}
        }

        EpollCmd::Poll
    }
}

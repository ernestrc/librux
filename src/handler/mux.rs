use error::*;

use handler::Handler;
use nix::sys::socket::*;

use nix::unistd::close as rclose;
use poll::*;
use protocol::*;
use slab::Slab;
use std::mem;
use std::os::unix::io::RawFd;

pub struct MuxEvent {
    pub kind: EpollEventKind,
    pub fd: RawFd,
}

pub enum MuxCmd {
    Clear,
    Keep,
}

pub struct SyncMux<'p, P: StaticProtocol<'p, MuxEvent, MuxCmd> + 'static> {
    epfd: EpollFd,
    handlers: Slab<P::H, usize>,
    protocol: P,
    interests: EpollEventKind,
}

impl<'p, P: StaticProtocol<'p, MuxEvent, MuxCmd> + Clone> Clone for SyncMux<'p, P> {
    fn clone(&self) -> Self {
        SyncMux {
            epfd: self.epfd,
            handlers: Slab::with_capacity(self.handlers.capacity()),
            protocol: self.protocol.clone(),
            interests: self.interests.clone(),
        }
    }
}

impl<'p, P: StaticProtocol<'p, MuxEvent, MuxCmd>> SyncMux<'p, P> {
    pub fn new(max_handlers: usize, interests: EpollEventKind, epfd: EpollFd, protocol: P) -> SyncMux<'p, P> {
        SyncMux {
            epfd: epfd,
            handlers: Slab::with_capacity(max_handlers),
            protocol: protocol,
            interests: interests,
        }
    }

    #[inline(always)]
    fn new_handler(
        protocol: &'p mut P,
        proto: Position<P::Protocol>,
        clifd: RawFd,
        interests: EpollEventKind,
        epfd: &EpollFd,
        handlers: &mut Slab<P::H, usize>
    ) -> Result<(usize, RawFd)> {
        match handlers.vacant_entry() {
            Some(entry) => {

                let i = entry.index();
                let action: Action<P> = Action::Notify(i, clifd);

                let interest = EpollEvent {
                    events: interests,
                    data: protocol.encode(action),
                };

                let h = protocol.get_handler(proto, *epfd, i);

                entry.insert(h);

                match epfd.register(clifd, &interest) {
                    Ok(_) => {}
                    Err(e) => {
                        error!("closing: {:?}", e);
                        perror!("{}", rclose(clifd));
                        return Err(e);
                    }
                };

                Ok((i, clifd))
            }
            None => Err("reached maximum number of handlers".into()),
        }
    }

    #[inline(always)]
    fn decode(
        protocol: &'p mut P,
        epfd: &EpollFd,
        event: EpollEvent,
        interests: EpollEventKind,
        handlers: &mut Slab<P::H, usize>
    ) -> Result<Option<(usize, RawFd)>> {

        let action: Action<P> = protocol.decode(event.data);
        match action {
            Action::New(proto, clifd) => {
                let (idx, fd) = Self::new_handler(protocol,
                                                  Position::Handler(proto),
                                                  clifd,
                                                  interests,
                                                  epfd,
                                                  handlers)?;
                Ok(Some((idx, fd)))
            }
            Action::Notify(i, fd) => Ok(Some((i, fd))),
            Action::NoAction(data) => {
                let srvfd = data as i32;
                match eintr!(accept, "accept", srvfd) {
                    Ok(Some(clifd)) => {
                        debug!("accept: accepted new tcp client {}", &clifd);
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
    where P: StaticProtocol<'p, MuxEvent, MuxCmd>,
{
    type In = EpollEvent;
    type Out = EpollCmd;

    fn update(&mut self, epfd: EpollFd) {
        self.epfd = epfd;
    }

    fn ready(&mut self, event: EpollEvent) -> EpollCmd {

        // FIXME: solve with associated lifetimes
        let proto: &'p mut P = unsafe { mem::transmute(&mut self.protocol) };

        let (idx, fd) = match SyncMux::decode(proto, &self.epfd, event, self.interests, &mut self.handlers) {
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

        match self.handlers[idx].ready(MuxEvent {
            kind: event.events,
            fd: fd,
        }) {
            MuxCmd::Clear => {
                let handler = self.handlers.remove(idx).unwrap();
                self.protocol.done(handler, idx);
            }
            _ => {}
        }

        EpollCmd::Poll
    }
}

use std::os::unix::io::RawFd;

use nix::unistd::close as rclose;
use nix::sys::socket::*;
use slab::Slab;

use handler::Handler;
use error::*;
use poll::*;
use protocol::*;

pub struct MuxEvent {
    pub kind: EpollEventKind,
    pub fd: RawFd,
}

pub enum MuxCmd {
    Clear,
    Keep,
}

pub struct SyncMux<'p, P: StaticProtocol<'p, MuxEvent, MuxCmd> + 'p> {
    epfd: EpollFd,
    handlers: Slab<P::H, usize>,
    protocol: &'p mut P,
    interests: EpollEventKind,
}

impl<'p, P: StaticProtocol<'p, MuxEvent, MuxCmd>> SyncMux<'p, P> {
    pub fn new(max_handlers: usize,
               interests: EpollEventKind,
               epfd: EpollFd,
               protocol: &'p mut P)
               -> SyncMux<'p, P> {
        SyncMux {
            epfd: epfd,
            handlers: Slab::with_capacity(max_handlers),
            protocol: protocol,
            interests: interests,
        }
    }

    #[inline]
    fn new_handler(protocol: &'p mut P,
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

                let action: Action<P> = Action::Notify(i, clifd);

                let interest = EpollEvent {
                    events: interests,
                    data: MuxProtocol::encode(action),
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
    fn decode(protocol: &'p mut P,
              epfd: &EpollFd,
              event: EpollEvent,
              interests: EpollEventKind,
              handlers: &mut Slab<P::H, usize>)
              -> Result<Option<(usize, RawFd)>> {

        let action: Action<P> = MuxProtocol::decode(event.data);
        match action {
            Action::New(proto, clifd) => {
                let (idx, fd) = Self::new_handler(protocol,
                                                  Position::Handler(proto),
                                                  clifd,
                                                  interests,
                                                  epfd,
                                                  handlers)
                    ?;
                Ok(Some((idx, fd)))
            }
            Action::Notify(i, fd) => Ok(Some((i, fd))),
            Action::NoAction(data) => {
                let srvfd = data as i32;
                match eintr!(accept, "accept", srvfd) {
                    Ok(Some(clifd)) => {
                        trace!("accept: accepted new tcp client {}", &clifd);
                        Self::new_handler(protocol,
                                          Position::Root,
                                          clifd,
                                          interests,
                                          epfd,
                                          handlers)
                            ?;
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

    #[inline]
    fn ready(&mut self, event: EpollEvent) -> EpollCmd {

        // FIXME
        let proto: &'p mut P = unsafe { ::std::mem::transmute_copy(&mut self.protocol) };

        let (idx, fd) =
            match SyncMux::decode(proto, &self.epfd, event, self.interests, &mut self.handlers) {
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

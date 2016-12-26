use RawFd;
use error::*;
use handler::*;
use nix::sys::socket::*;
use poll::*;
use slab::Slab;

#[derive(Debug, Clone, PartialEq)]
pub enum MuxCmd {
  Close,
  Keep,
}

#[derive(Debug)]
pub struct SyncMux<'p, P: HandlerFactory<'p, EpollEvent, MuxCmd> + 'static> {
  epfd: EpollFd,
  handlers: Slab<P::H, usize>,
  factory: P,
  interests: EpollEventKind,
}

impl<'p, P> SyncMux<'p, P>
  where P: HandlerFactory<'p, EpollEvent, MuxCmd> + 'static,
{
  pub fn new(max_handlers: usize, interests: EpollEventKind, epfd: EpollFd, factory: P)
             -> SyncMux<'p, P> {
    SyncMux {
      epfd: epfd,
      handlers: Slab::with_capacity(max_handlers),
      factory: factory,
      interests: interests,
    }
  }

  #[inline(always)]
  fn new_handler(factory: &'p mut P, clifd: RawFd, interests: EpollEventKind, epfd: &EpollFd,
                 handlers: &mut Slab<P::H, usize>)
                 -> Result<(usize, RawFd)> {
    match handlers.vacant_entry() {
      Some(entry) => {

        let i = entry.index();

        let interest = EpollEvent {
          events: interests,
          data: Action::encode(Action::Notify(i, clifd)),
        };

        let h = factory.new(*epfd, i, clifd);

        entry.insert(h);

        match epfd.register(clifd, &interest) {
          Ok(_) => {}
          Err(e) => {
            ::close(clifd).ok();
            return Err(e);
          }
        };

        Ok((i, clifd))
      }
      None => Err("reached maximum number of handlers".into()),
    }
  }

  #[inline(always)]
  fn decode(factory: &'p mut P, epfd: &EpollFd, event: EpollEvent, interests: EpollEventKind,
            handlers: &mut Slab<P::H, usize>)
            -> Result<Option<(usize, RawFd)>> {

    match Action::decode(event.data) {
      Action::Notify(i, fd) => Ok(Some((i, fd))),
      Action::New(data) => {
        let srvfd = data as i32;
        match eintr!(accept, "accept", srvfd) {
          Ok(Some(clifd)) => {
            debug!("accept: accepted new tcp client {}", &clifd);
            Self::new_handler(factory, clifd, interests, epfd, handlers)?;
            Ok(None)
          }
          Ok(None) => Err("accept4: socket not ready".into()),
          Err(e) => Err(e.into()),
        }
      }
    }
  }
}

impl<'p, P> Clone for SyncMux<'p, P>
  where P: HandlerFactory<'p, EpollEvent, MuxCmd> + Clone,
{
  fn clone(&self) -> Self {
    SyncMux {
      epfd: self.epfd,
      handlers: Slab::with_capacity(self.handlers.capacity()),
      factory: self.factory.clone(),
      interests: self.interests.clone(),
    }
  }
}

impl<'p, P> Reset for SyncMux<'p, P>
  where P: HandlerFactory<'p, EpollEvent, MuxCmd>,
{
  fn reset(&mut self, epfd: EpollFd) {
    self.epfd = epfd;
  }
}

impl<'p, P> Handler<EpollEvent, EpollCmd> for SyncMux<'p, P>
  where P: HandlerFactory<'p, EpollEvent, MuxCmd>,
{
  fn on_next(&mut self, mut event: EpollEvent) -> EpollCmd {

    // FIXME: solve with associated lifetimes
    let proto: &'p mut P = unsafe { ::std::mem::transmute(&mut self.factory) };

    let (idx, fd) =
      match SyncMux::decode(proto, &self.epfd, event, self.interests, &mut self.handlers) {
        Ok(Some((idx, fd))) => (idx, fd),
        Ok(None) => {
          // new connection
          return EpollCmd::Poll;
        }
        Err(e) => {
          report_err!("{}", e);
          return EpollCmd::Poll;
        }
      };

    // FIXME: not optimal
    match self.handlers.entry(idx) {
      Some(mut entry) => {
        event.data = fd as u64;
        match entry.get_mut().on_next(event) {
          MuxCmd::Close => {
            perror!("unistd::close", ::close(fd));
            let handler = entry.remove();
            self.factory.done(handler, idx);
          }
          _ => {}
        }
      }
      None => warn!("received EpollEvent for handler at index {:?}, but no handler found", idx),
    }
    EpollCmd::Poll
  }
}

pub enum Action {
  Notify(usize, RawFd),
  New(u64),
}

impl Action {
  #[inline(always)]
  pub fn encode(action: Action) -> u64 {
    match action {
      Action::Notify(data, fd) => ((fd as u64) << 31) | ((data as u64) << 15) | 0,
      Action::New(data) => data,
    }
  }

  #[inline(always)]
  pub fn decode(data: u64) -> Action {
    let arg1 = ((data >> 15) & 0xffff) as usize;
    let fd = (data >> 31) as i32;
    match data & 0x7fff {
      0 => Action::Notify(arg1, fd),
      _ => Action::New(data),
    }
  }
}

#[macro_export]
macro_rules! keep {
  ($cmd:expr) => {{
    match $cmd {
      e @ MuxCmd::Close => return e,
      _ => {}
    }
  }}
}

#[cfg(test)]
mod tests {
  use super::Action;
  #[test]
  fn decode_encode_new_action() {
    let data = Action::encode(Action::New(::std::u64::MAX));

    if let Action::New(fd) = Action::decode(data) {
      assert!(fd == ::std::u64::MAX);
    } else {
      panic!("action is not Action::New")
    }
  }

  #[test]
  fn decode_encode_notify_action() {
    let data = Action::encode(Action::Notify(10110, 0));

    if let Action::Notify(data, fd) = Action::decode(data) {
      assert!(data == 10110);
      assert!(fd == 0);
    } else {
      panic!("action is not Action::Notify")
    }
  }
}

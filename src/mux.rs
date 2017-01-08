use RawFd;
use error::*;
use handler::*;
use nix::sys::socket::*;
use poll::*;
use slab::Slab;

#[derive(Debug)]
pub struct SyncMux<'mux, H, P: HandlerFactory<'mux, H, EpollEvent, MuxCmd> + 'mux>
  where H: Handler<EpollEvent, MuxCmd>, // TODO + HandlerProp::get_interests()
{
  epfd: EpollFd,
  handlers: Slab<H, usize>,
  resources: Vec<P::Resource>,
  factory: P,
  interests: EpollEventKind,
  _marker: ::std::marker::PhantomData<&'mux ()>,
}

impl<'mux, H, P> SyncMux<'mux, H, P>
  where H: Handler<EpollEvent, MuxCmd>,
        P: HandlerFactory<'mux, H, EpollEvent, MuxCmd> + 'static,
{
  pub fn new(max_handlers: usize, interests: EpollEventKind, epfd: EpollFd, factory: P)
             -> SyncMux<'mux, H, P> {
    SyncMux {
      epfd: epfd,
      handlers: Slab::with_capacity(max_handlers),
      resources: vec!(Default::default(); max_handlers),
      factory: factory,
      interests: interests,
      _marker: ::std::marker::PhantomData {},
    }
  }
}

impl<'mux, H, P> Clone for SyncMux<'mux, H, P>
  where H: Handler<EpollEvent, MuxCmd>,
        P: HandlerFactory<'mux, H, EpollEvent, MuxCmd> + Clone + 'static,
{
  fn clone(&self) -> Self {
    SyncMux {
      epfd: self.epfd,
      handlers: Slab::with_capacity(self.handlers.capacity()),
      resources: vec!(Default::default(); self.handlers.capacity()),
      factory: self.factory.clone(),
      interests: self.interests.clone(),
      _marker: ::std::marker::PhantomData {},
    }
  }
}

impl<'mux, H, P> Reset for SyncMux<'mux, H, P>
  where H: Handler<EpollEvent, MuxCmd>,
        P: HandlerFactory<'mux, H, EpollEvent, MuxCmd> + 'static,
{
  fn reset(&mut self, epfd: EpollFd) {
    self.epfd = epfd;
  }
}

#[macro_export]
macro_rules! keep_or {
  ($cmd:expr, $b: block) => {{
    match $cmd {
      MuxCmd::Close => $b,
      _ => (),
    }
  }}
}

#[macro_export]
macro_rules! keep {
  ($cmd:expr) => {{
    keep_or_return!($cmd, MuxCmd::Close)
  }}
}

#[macro_export]
macro_rules! keep_or_return {
  ($cmd:expr, $ret: expr) => {{
    keep_or!($cmd, { return $ret; });
  }}
}

macro_rules! keep_or_close {
  ($cmd:expr, $clifd: expr) => {{
    keep_or!($cmd, { close!($clifd); });
  }}
}

macro_rules! close {
  ($clifd: expr) => {{
    perror!("unistd::close", ::close($clifd));
    return EpollCmd::Poll;
  }}
}

macro_rules! ok_or_close {
  ($cmd:expr, $clifd: expr) => {{
    match $cmd {
      Err(e) => {
        error!("{}", e);
        close!($clifd);
      },
      Ok(res) => res,
    }
  }}
}

macro_rules! some_or_close {
  ($cmd:expr, $clifd: expr) => {{
    match $cmd {
      None => {
        close!($clifd);
      },
      Some(res) => res,
    }
  }}
}

impl<'mux, H, P> Handler<EpollEvent, EpollCmd> for SyncMux<'mux, H, P>
  where H: Handler<EpollEvent, MuxCmd>,
        P: HandlerFactory<'mux, H, EpollEvent, MuxCmd> + 'static,
{
  fn on_next(&mut self, mut event: EpollEvent) -> EpollCmd {

    match Action::decode(event.data) {

      Action::Notify(i, clifd) => {
        let mut entry = some_or_close!(self.handlers.entry(i), clifd);

        event.data = clifd as u64;

        keep_or!(entry.get_mut().on_next(event), {
          // self.factory.release(entry.remove(), i);
          entry.remove();
          close!(clifd);
        });
      }

      Action::New(data) => {
        let srvfd = data as i32;
        match eintr!(accept, "accept", srvfd) {
          Ok(Some(clifd)) => {
            debug!("accept: accepted new tcp client {}", &clifd);
            // TODO grow slab, deprecate max_conn in favour of reserve slots
            // or Mux::reserve to pre-allocate and then grow as it needs more
            let entry = some_or_close!(self.handlers.vacant_entry(), clifd);
            let i = entry.index();

            let mut h = unsafe {
              self.factory.new(self.epfd, clifd, &mut *(&mut self.resources[i] as *mut P::Resource))
            };

            let mut event = EpollEvent {
              events: self.interests,
              data: clifd as u64,
            };

            keep_or_close!(h.on_next(event), clifd);

            event.data = Action::encode(Action::Notify(i, clifd));
            ok_or_close!(self.epfd.register(clifd, &event), clifd);

            entry.insert(h);
          }
          Ok(None) => debug!("accept4: socket not ready"),
          Err(e) => report_err!("accept4: {}", e.into()),
        }
      }
    };

    EpollCmd::Poll
  }
}

#[derive(Debug, Clone, PartialEq)]
pub enum MuxCmd {
  Close,
  Keep,
}

impl Default for MuxCmd {
  fn default() -> MuxCmd {
    MuxCmd::Keep
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

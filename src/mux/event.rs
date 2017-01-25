use RawFd;
use epoll::EpollEventKind;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MuxCmd {
  Close,
  Keep,
}

impl Default for MuxCmd {
  fn default() -> MuxCmd {
    MuxCmd::Keep
  }
}

pub struct MuxEvent<'r, R: 'r> {
  pub resource: &'r mut R,
  pub events: EpollEventKind,
  pub fd: RawFd,
}

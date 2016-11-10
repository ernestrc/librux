use std::os::unix::io::RawFd;
use std::boxed::Box;

use poll::{EpollFd, EpollEvent};
use handler::Handler;

pub type HandlerId = usize;

pub enum Action<P: IOProtocol> {
    New(P::Protocol, RawFd),
    Notify(HandlerId, RawFd),
    NoAction(u64),
}

pub trait StaticProtocol<H: Handler<EpollEvent>>
    where Self: IOProtocol {
    fn get_handler(&self, p: Self::Protocol, epfd: EpollFd, id: usize) -> H;
}

pub trait DynamicProtocol 
    where Self: IOProtocol {

    fn get_handler(&self, p: Self::Protocol, epfd: EpollFd, id: usize) -> Box<Handler<EpollEvent>>;

}

pub trait IOProtocol
    where Self: Sized + Send + Copy
{
    type Protocol: From<usize> + Into<usize> + Copy;

    fn encode(&self, action: Action<Self>) -> u64 {
        match action {
            Action::Notify(id, fd) => ((fd as u64) << 31) | ((id as u64) << 15) | 0,
            Action::New(protocol, fd) => {
                let protocol: usize = protocol.into();
                ((fd as u64) << 31) | ((protocol as u64) << 15) | 1
            }
            Action::NoAction(data) => data,
        }
    }

    fn decode(&self, data: u64) -> Action<Self> {
        let arg1 = ((data >> 15) & 0xffff) as usize;
        let fd = (data >> 31) as i32;
        match data & 0x7fff {
            0 => Action::Notify(arg1, fd),
            1 => Action::New(From::from(arg1), fd),
            _ => Action::NoAction(data),
        }
    }
}

#[cfg(test)]
mod tests {
    use poll::*;
    use handler::Handler;
    use super::*;

    #[derive(Clone, Copy)]
    struct TestIOProtocol;

    const PROTO1: usize = 1;

    struct TestHandler {
        on_close: bool,
        on_error: bool,
        on_readable: bool,
        on_writable: bool,
    }

    impl Handler<EpollEvent> for TestHandler {
        fn ready(&mut self, e: &EpollEvent) {
            let kind = e.events;

            if kind.contains(EPOLLERR) {
                self.on_error = true;
            }

            if kind.contains(EPOLLIN) {
                self.on_readable = true;
            }

            if kind.contains(EPOLLOUT) {
                self.on_writable = true;
            }

            if kind.contains(EPOLLHUP) {
                self.on_close = true;
            }
        }
        fn is_terminated(&self) -> bool {
            false
        }
    }

    impl IOProtocol for TestIOProtocol {
        type Protocol = usize;
    }

    impl DynamicProtocol for TestIOProtocol {
        fn get_handler(&self, _: Self::Protocol, _: EpollFd, _: usize) -> Box<Handler<EpollEvent>> {
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
        let test = TestIOProtocol;
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
        let test = TestIOProtocol;
        let data = test.encode(Action::Notify(10110, 0));

        if let Action::Notify(id, fd) = test.decode(data) {
            assert!(id == 10110);
            assert!(fd == 0);
        } else {
            panic!("action is not Action::Notify")
        }
    }
}

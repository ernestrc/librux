use std::os::unix::io::RawFd;

use poll::EpollFd;
use handler::Handler;

pub enum Action<P: MuxProtocol> {
    New(P::Protocol, RawFd),
    Notify(usize, RawFd),
    NoAction(u64),
}

pub enum Position<P> {
    Root,
    Handler(P),
}

pub trait StaticProtocol<'p, In, Out>
    where Self: MuxProtocol + 'p
{
    type H: Handler<In = In, Out = Out>;

    fn done(&mut self, handler: Self::H, index: usize);

    fn get_handler(&'p mut self, p: Position<Self::Protocol>, epfd: EpollFd, id: usize) -> Self::H;
}

pub trait MuxProtocol
    where Self: Sized
{
    type Protocol: From<usize> + Into<usize>;

    #[inline]
    fn encode(&self, action: Action<Self>) -> u64 {
        match action {
            Action::Notify(data, fd) => ((fd as u64) << 31) | ((data as u64) << 15) | 0,
            Action::New(protocol, fd) => {
                let protocol: usize = protocol.into();
                ((fd as u64) << 31) | ((protocol as u64) << 15) | 1
            }
            Action::NoAction(data) => data,
        }
    }

    #[inline]
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

    #[derive(Clone)]
    struct TestMuxProtocol;

    const PROTO1: usize = 1;

    struct TestHandler {
        on_close: bool,
        on_error: bool,
        on_readable: bool,
        on_writable: bool,
    }

    impl Handler for TestHandler {
        type In = EpollEvent;
        type Out = EpollCmd;

        fn ready(&mut self, e: EpollEvent) -> EpollCmd {
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

            EpollCmd::Poll
        }
    }

    impl MuxProtocol for TestMuxProtocol {
        type Protocol = usize;
    }

    impl<'p> StaticProtocol<EpollEvent, EpollCmd> for TestMuxProtocol {
        type H = TestHandler;
        fn done(&mut self, handler: Self::H, index: usize) {}

        fn get_handler(&mut self, _: Position<usize>, _: EpollFd, _: usize) -> TestHandler {
            TestHandler {
                on_close: false,
                on_error: false,
                on_readable: false,
                on_writable: false,
            }
        }
    }

    #[test]
    fn decode_encode_new_action() {
        let test = TestMuxProtocol;
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
        let test = TestMuxProtocol;
        let data = test.encode(Action::Notify(10110, 0));

        if let Action::Notify(data, fd) = test.decode(data) {
            assert!(data == 10110);
            assert!(fd == 0);
        } else {
            panic!("action is not Action::Notify")
        }
    }
}

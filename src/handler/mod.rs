pub mod mux;

pub trait Handler<'h> {
    type In;
    type Out;
    fn reset(&'h mut self);
    fn ready(&'h mut self, Self::In) -> Option<Self::Out>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use poll::*;
    use protocol::*;
    use std::os::unix::io::{RawFd, AsRawFd};

    /// --------------------------------------
    /// 1
    pub struct Usize<'h, H: Handler<'h> + 'h> {
        counter: usize,
        next: &'h mut H,
    }

    impl<'h, 'r: 'h, H: Handler<'h, In = i32, Out = u32>> Handler<'r> for Usize<'h, H> {
        type In = usize;
        type Out = isize;

        fn reset(&mut self) {
            self.counter = 0;
        }

        fn ready(&'r mut self, event: usize) -> Option<isize> {
            if self.counter > 5 || event >= 5 {
                return self.next.ready(event as i32).map(|s| s as isize);
            }
            self.counter += event;
            None
        }
    }
    /// --------------------------------------
    /// 2
    pub struct I32 {
        counter: i32,
    }

    impl<'h> Handler<'h> for I32 {
        type In = i32;
        type Out = u32;

        fn reset(&mut self) {
            self.counter = 0;
        }

        fn ready(&mut self, event: i32) -> Option<u32> {
            if self.counter > 5 || event >= 5 {
                return Some((self.counter + event) as u32);
            }

            self.counter += event;
            None
        }
    }

    #[test]
    fn it_works() {
        let mut u = Usize {
            counter: 0,
            next: &mut I32 { counter: 0 },
        };

        assert!(u.ready(1) == None);
        // assert!(u.ready(1) == None);
        // assert!(u.ready(1) == None);
        // assert!(u.ready(1) == None);
        // assert!(u.ready(1) == None);
        // assert_eq!(u.ready(5), Some(5));
    }

    /// --------------------------------------
    /// 3
    pub struct TTT<'b> {
        s: &'b usize,
    }

    struct RRR<'h, 'p, 'r, P>
        where 'p: 'h,
              'r: 'h + 'p,
              P: StaticProtocol<'h, 'p, TTT<'h>, TTT<'h>> + 'p
    {
        s: &'h usize,
        p: &'p mut P,
        _marker3: ::std::marker::PhantomData<&'r bool>,
        handlers: Vec<P::H>,
    }

    struct HHH<'b> {
        p: &'b usize,
    }

    impl<'h, 'p: 'h, 'r: 'h, P> Handler<'r> for RRR<'h, 'p, 'r, P>
        where P: StaticProtocol<'h, 'p, TTT<'h>, TTT<'h>> + 'p
    {
        type In = usize;
        type Out = usize;

        fn reset(&'r mut self) {}

        fn ready(&'r mut self, _: usize) -> Option<usize> {
            let epfd = EpollFd::new(0 as RawFd);
            let msg = TTT { s: self.s };
            let idx = self.handlers.len();
            self.handlers.push(self.p.get_handler(Position::Root, epfd, 0));
            self.handlers[idx].ready(msg).map(|s| *s.s)
        }
    }

    impl<'b> Handler<'b> for HHH<'b> {
        type In = TTT<'b>;
        type Out = TTT<'b>;

        fn reset(&'b mut self) {}

        fn ready(&'b mut self, event: TTT<'b>) -> Option<TTT<'b>> {
            Some(event)
        }
    }

    #[derive(Clone, Copy, Debug)]
    struct TestProtocol {
        prop: usize,
    }

    impl MuxProtocol for TestProtocol {
        type Protocol = usize;
    }

    impl<'h, 'proto: 'h> StaticProtocol<'h, 'proto, TTT<'h>, TTT<'h>> for TestProtocol {
        type H = HHH<'h>;

        fn get_handler(&'proto mut self,
                       p: Position<Self::Protocol>,
                       _: EpollFd,
                       _: usize)
                       -> Self::H {

            HHH { p: &self.prop }
        }
    }

    #[test]
    fn test_1() {
        let mut p = TestProtocol { prop: 0 };
        let epfd = EpollFd::new(0 as RawFd);
        let prop = 1_usize;

        {
            let mut handler = p.get_handler(Position::Root, epfd, 0);

            {
                let t = TTT { s: &prop };

                handler.ready(t);
            }
        }
    }
}

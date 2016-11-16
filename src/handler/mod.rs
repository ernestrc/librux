pub mod mux;

pub trait Handler {
    type In;
    type Out;
    fn reset(&mut self);
    fn ready(&mut self, Self::In) -> Option<Self::Out>;
}

#[cfg(test)]
mod tests {
    pub struct Usize<H: Handler> {
        counter: usize,
        next: H,
    }

    impl<H: Handler<In = i32, Out = u32>> Handler for Usize<H> {
        type In = usize;
        type Out = isize;

        fn reset(&mut self) {
            self.counter = 0;
        }

        fn ready(&mut self, event: usize) -> Option<isize> {
            if self.counter > 5 || event >= 5 {
                return self.next.ready(event as i32).map(|s| s as isize);
            }
            self.counter += event;
            None
        }
    }

    pub struct I32 {
        counter: i32,
    }

    impl Handler for I32 {
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

    use super::*;

    #[test]
    fn it_works() {
        let mut u = Usize {
            counter: 0,
            next: I32 { counter: 0 },
        };

        assert!(u.ready(1) == None);
        assert!(u.ready(1) == None);
        assert!(u.ready(1) == None);
        assert!(u.ready(1) == None);
        assert!(u.ready(1) == None);
        assert_eq!(u.ready(5), Some(5));
    }
}

// #![feature(alloc_system)]
// extern crate alloc_system;

extern crate rux;

use rux::buf::ByteBuffer;

const SIZE: usize = 1024 * 1024 * 1024;
const CHUNK: usize = 1024 * 1024;

#[inline(never)]
fn main() {
    let iter: usize = SIZE / CHUNK;

    println!("copying {} {} times ({}), ", CHUNK, iter, SIZE);

    let mut buf = ByteBuffer::with_capacity(SIZE);

    for _ in 0..iter {
        let slice = [255; CHUNK];
        {
            buf.write(&slice).unwrap();
        }
        assert_eq!(buf.readable(), CHUNK);

        let mut slice2 = [0_u8; CHUNK];
        {
            let cnt = buf.read(&mut slice2).unwrap();
            buf.consume(cnt);
        }
        assert_eq!(buf.readable(), 0);
    };
}

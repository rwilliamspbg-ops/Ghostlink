#![no_main]

use libfuzzer_sys::fuzz_target;
use ghostlink_core::protocol::Frame;

fuzz_target!(|data: &[u8]| {
    let _ = Frame::decode(data);
});

#![no_main]

use libfuzzer_sys::fuzz_target;
use ghostlink_core::protocol::DiscoveryFrame;

fuzz_target!(|data: &[u8]| {
    let _ = DiscoveryFrame::decode(data);
});

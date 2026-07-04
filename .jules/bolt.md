## 2026-07-02 - [SPSC Ring Buffer Optimization]
**Learning:** Shared atomic counters in SPSC (Single-Producer Single-Consumer) structures cause significant cache line contention (false sharing or simply bouncing) even if the rest of the structure is properly padded. The length of a ring buffer can be derived from head/tail pointers, making an explicit shared 'count' or 'empty_count' redundant and slow.
**Action:** Always derive length/fullness from head and tail pointers in SPSC buffers instead of using a shared atomic counter. Ensure power-of-two capacities to allow bitwise masking for wrapping.

## 2026-07-02 - [TCP Transport Buffer Optimization]
**Learning:** High-throughput TCP transport can be bottlenecked by intermediate buffer copies and zero-initialization of vectors. Using stack-allocated headers and writing directly to a  avoids large intermediate memory copies. For  payloads on little-endian systems, reading directly into uninitialized  (using  and ) eliminates the significant cost of zero-initialization.
**Action:** Minimize intermediate buffers in hot paths. Use  and  when reading POD data from network if performance is critical.

## 2026-07-02 - [TCP Transport Buffer Optimization]
**Learning:** High-throughput TCP transport can be bottlenecked by intermediate buffer copies and zero-initialization of vectors. Using stack-allocated headers and writing directly to a `BufWriter` avoids large intermediate memory copies. For `f32` payloads on little-endian systems, reading directly into uninitialized `Vec<f32>` (using `unsafe` and `set_len`) eliminates the significant cost of zero-initialization.
**Action:** Minimize intermediate buffers in hot paths. Use `Vec::with_capacity` and `unsafe { set_len }` when reading POD data from network if performance is critical.

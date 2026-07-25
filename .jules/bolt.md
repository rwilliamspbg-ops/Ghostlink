## 2026-07-02 - [SPSC Ring Buffer Optimization]
**Learning:** Shared atomic counters in SPSC (Single-Producer Single-Consumer) structures cause significant cache line contention (false sharing or simply bouncing) even if the rest of the structure is properly padded. The length of a ring buffer can be derived from head/tail pointers, making an explicit shared 'count' or 'empty_count' redundant and slow.
**Action:** Always derive length/fullness from head and tail pointers in SPSC buffers instead of using a shared atomic counter. Ensure power-of-two capacities to allow bitwise masking for wrapping.

## 2026-07-02 - [TCP Transport Buffer Optimization]
**Learning:** High-throughput TCP transport can be bottlenecked by intermediate buffer copies and zero-initialization of vectors. Using stack-allocated headers and writing directly to a `BufWriter` avoids large intermediate memory copies. For `f32` payloads on little-endian systems, reading directly into `MaybeUninit` memory and then transmuting to `Vec<f32>` eliminates the significant cost of zero-initialization without creating UB references to uninitialized memory.
**Action:** Minimize intermediate buffers in hot paths. Use `MaybeUninit` and `transmute` (for POD types like `f32`) when reading large datasets from the network if performance is critical.

## 2026-07-02 - [Binary Protocol Serialization Optimization]
**Learning:** Pre-allocating a single `Vec` of the exact final capacity and serializing struct payloads directly into it via an `encode_payload_into` method avoids the overhead of intermediate heap allocations and memory copies. Additionally, avoiding `copy_from_slice` in low-level header encoding by assigning elements directly to fixed indices eliminates bounds checks and improves instruction pipelining.
**Action:** Always provide an `encode_into` style method for high-performance binary structures to allow zero-copy/zero-allocation serialization directly into target buffers or frames. Avoid bounds checking in short fixed-size array writes by indexing them directly with constant offsets instead of using slice copy methods.

## 2026-07-02 - [UTF-8 Decoding Optimization]
**Learning:** In hot-path binary packet deserialization, allocating intermediate byte vectors (`payload.to_vec()`) just to validate them using `String::from_utf8` introduces significant heap allocation and garbage collection/deallocation overhead. Instead, using `std::str::from_utf8` to validate the byte slice *in-place* (zero-copy) and only allocating once (`.to_string()`) on successful validation reduces decode/round-trip latency and yields ~7.3% higher throughput.
**Action:** Always validate byte slices in-place using `std::str::from_utf8` (or `std::str::from_utf8_mut`) before allocating owned `String` instances in deserialization paths.


## 2026-07-02 - [Cluster Health Query Optimization]
**Learning:** Redundant iterations and nested metric lookups on thread-safe collections (like `ClusterState::get_metrics`) introduce unnecessary mutex acquisition overhead and expensive clones of large status structs (such as `NodeMetrics`). Consolidating query loops with functional combinators like `filter_map` reduces lock contention and halves cloning/lookup overhead.
**Action:** When querying statuses or counting/collecting across thread-safe shared maps or lists, perform lookups once per element, derive any aggregations directly, and avoid secondary lookups or passes.

## 2026-07-02 - [Cluster Health Lock & Clone Overhead Elimination]
**Learning:** In cluster health calculations, calling multiple helper methods on `ClusterState` (e.g., `active_nodes`, `nodes_snapshot`, `get_metrics`) leads to acquiring the internal metrics mutex multiple times per call and performing numerous expensive deep clones of `NodeMetrics`. Exposing crate-private (`pub(crate)`) access to the underlying metrics map allows acquiring the mutex exactly once and performing a single linear iteration. This bypasses all allocation, cloning, and redundant lock acquisition overhead completely.
**Action:** Consolidate multiple metrics lookup sweeps into a single-lock, zero-clone traversal. Expose inner collections internally (using `pub(crate)`) when complex multi-metric queries can be performed in a single lock acquisition rather than invoking multiple smaller methods.

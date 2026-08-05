//! GPU buffer wrapper with dynamic resizing.

use encase::{ShaderSize, ShaderType, internal::WriteInto};
use iced_wgpu::wgpu::{self, BindingResource};

const BUFFER_GROWTH_FACTOR: f32 = 1.5;
const BUFFER_MIN_ITEMS: usize = 16;

/// GPU buffer wrapper with incremental update support.
///
/// Manages a GPU storage buffer alongside a CPU-side Vec mirror. The GPU buffer
/// grows like a Vec (factor 1.5x) and is never shrunk, so steady-state frames
/// after the first few cause zero GPU allocations.
///
/// PERSISTENT write model (idle-skip groundwork): `clear()` only rewinds the live
/// length to 0; it does NOT drop the CPU mirror or the GPU data. Each frame's
/// writes OVERWRITE from slot 0 via `push`/`push_bulk`, so the previous frame's
/// contents survive in `buffer_vec` for change detection and a skipped write
/// leaves valid data in place. `live_len` is the count written this frame; slots
/// past it are stale but never read (consumers bound their reads by `len()`),
/// which also makes a shrinking frame truncate for free.
pub(crate) struct Buffer<T> {
    buffer_wgpu: wgpu::Buffer,
    /// CPU mirror. May hold MORE than `live_len` items (the high-water mark of any
    /// frame); only `[..live_len]` is live this frame.
    buffer_vec: Vec<T>,
    /// Items written this frame (the logical length). `len()` returns this.
    live_len: usize,
    scratch: Vec<u8>,
    label: Option<&'static str>,
    usage: wgpu::BufferUsages,
    generation: u64,
    /// Bytes handed to `queue.write_buffer` since the last `take_written_bytes`.
    written_bytes: u64,
    /// Hard ceiling for this binding, from the device's
    /// `max_storage_buffer_binding_size`, rounded down to a 4-byte multiple.
    /// Growth clamps here (see [`grown_size`]); a write that would exceed it is
    /// SKIPPED and counted in `dropped_items`.
    max_bytes: u64,
    /// Items never uploaded because the binding ceiling was reached. Nonzero
    /// means the scene exceeds what this device can bind: the excess geometry
    /// is absent from the frame, and `live_len` never advances over it, so
    /// consumers bounded by `len()` cannot read the missing slots.
    dropped_items: u64,
}

impl<T: ShaderSize> Buffer<T> {
    pub fn new(
        device: &wgpu::Device,
        label: Option<&'static str>,
        usage: wgpu::BufferUsages,
    ) -> Self {
        let item_size = T::SHADER_SIZE.get() as usize;
        let size = (BUFFER_MIN_ITEMS * item_size) as wgpu::BufferAddress;
        let buffer_wgpu = create_wgpu_buffer(device, label, size, usage);
        let buffer_vec = Vec::with_capacity(BUFFER_MIN_ITEMS);
        Self {
            buffer_wgpu,
            buffer_vec,
            live_len: 0,
            scratch: Vec::new(),
            label,
            usage,
            generation: 0,
            written_bytes: 0,
            max_bytes: u64::from(device.limits().max_storage_buffer_binding_size) & !3,
            dropped_items: 0,
        }
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Bytes handed to `queue.write_buffer` since the last call, then resets the counter.
    pub fn take_written_bytes(&mut self) -> u64 {
        std::mem::take(&mut self.written_bytes)
    }

    /// Size of the live GPU allocation in bytes (capacity, not live length).
    pub fn gpu_bytes(&self) -> u64 {
        self.buffer_wgpu.size()
    }

    pub fn as_entire_binding(&self) -> BindingResource<'_> {
        self.buffer_wgpu.as_entire_binding()
    }

    pub fn len(&self) -> usize {
        self.live_len
    }

    /// Items this binding can ever hold, from the device's storage-binding
    /// limit. Pushing past it drops the item (see [`Buffer::dropped_items`]).
    // Read only by the arena-ceiling test; not `cfg(test)` so the doc link above resolves.
    #[allow(dead_code)]
    pub fn capacity_items(&self) -> usize {
        (self.max_bytes / T::SHADER_SIZE.get()) as usize
    }

    /// Items never uploaded because [`Buffer::capacity_items`] was exceeded.
    pub fn dropped_items(&self) -> u64 {
        self.dropped_items
    }
    /// Allocation size for `required` bytes; see [`grown_size`].
    fn grown_size(&self, required: u64, item_size: u64) -> u64 {
        grown_size(required, item_size, self.max_bytes)
    }

    #[must_use]
    pub fn push(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, item: T) -> usize
    where
        T: ShaderType + ShaderSize + WriteInto,
    {
        let slot = self.live_len;
        let item_size = T::SHADER_SIZE.get() as usize;
        // Capacity check BEFORE any mutation: on refusal `live_len` and the CPU
        // mirror are untouched, so the slot is simply never live.
        if ((slot + 1) * item_size) as u64 > self.max_bytes {
            self.dropped_items += 1;
            return slot;
        }
        if slot < self.buffer_vec.len() {
            self.buffer_vec[slot] = item;
        } else {
            self.buffer_vec.push(item);
        }
        self.live_len += 1;

        let offset = slot * item_size;
        let required_size = self.live_len * item_size;

        if self.buffer_wgpu.size() < required_size as u64 {
            let new_size = self.grown_size(required_size as u64, item_size as u64);
            self.buffer_wgpu = create_wgpu_buffer(device, self.label, new_size, self.usage);
            self.generation += 1;
            self.rewrite_all(queue);
        } else {
            self.scratch.clear();
            self.scratch.resize(item_size, 0);
            let mut writer = encase::StorageBuffer::new(&mut self.scratch[..]);
            writer
                .write(&self.buffer_vec[slot])
                .expect("Failed to write to storage buffer");
            queue.write_buffer(&self.buffer_wgpu, offset as u64, &self.scratch);
            self.written_bytes += self.scratch.len() as u64;
        }
        slot
    }

    fn rewrite_all(&mut self, queue: &wgpu::Queue)
    where
        T: ShaderType + ShaderSize + WriteInto,
    {
        if self.live_len == 0 {
            return;
        }

        let item_size = T::SHADER_SIZE.get() as usize;
        let total_size = self.live_len * item_size;
        self.scratch.clear();
        self.scratch.resize(total_size, 0);

        for (i, item) in self.buffer_vec[..self.live_len].iter().enumerate() {
            let offset = i * item_size;
            let slice = &mut self.scratch[offset..offset + item_size];
            let mut writer = encase::StorageBuffer::new(slice);
            writer
                .write(item)
                .expect("Failed to write to storage buffer");
        }
        queue.write_buffer(&self.buffer_wgpu, 0, &self.scratch);
        self.written_bytes += self.scratch.len() as u64;
    }

    #[must_use]
    pub fn push_bulk(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, items: &[T]) -> usize
    where
        T: ShaderType + ShaderSize + WriteInto + Clone,
    {
        if items.is_empty() {
            return self.live_len;
        }

        let start_slot = self.live_len;
        let item_size = T::SHADER_SIZE.get() as usize;
        // Capacity check BEFORE any mutation (see `push`). The batch is
        // all-or-nothing: a partially uploaded run would leave the tail slots
        // holding a previous frame's bytes, which renders as stale geometry
        // rather than absent geometry.
        if ((start_slot + items.len()) * item_size) as u64 > self.max_bytes {
            self.dropped_items += items.len() as u64;
            return start_slot;
        }
        // Overwrite the live slots in place; extend the mirror only past its
        // high-water mark so prior-frame allocation is reused, not regrown.
        let overwrite = items
            .len()
            .min(self.buffer_vec.len().saturating_sub(start_slot));
        self.buffer_vec[start_slot..start_slot + overwrite].clone_from_slice(&items[..overwrite]);
        if overwrite < items.len() {
            self.buffer_vec.extend_from_slice(&items[overwrite..]);
        }
        self.live_len = start_slot + items.len();

        let required_size = self.live_len * item_size;

        if self.buffer_wgpu.size() < required_size as u64 {
            let new_size = self.grown_size(required_size as u64, item_size as u64);
            self.buffer_wgpu = create_wgpu_buffer(device, self.label, new_size, self.usage);
            self.generation += 1;
            self.rewrite_all(queue);
        } else {
            let total_write = items.len() * item_size;
            let offset = start_slot * item_size;
            self.scratch.clear();
            self.scratch.resize(total_write, 0);

            for (i, item) in items.iter().enumerate() {
                let item_offset = i * item_size;
                let slice = &mut self.scratch[item_offset..item_offset + item_size];
                let mut writer = encase::StorageBuffer::new(slice);
                writer
                    .write(item)
                    .expect("Failed to write to storage buffer");
            }
            queue.write_buffer(&self.buffer_wgpu, offset as u64, &self.scratch);
            self.written_bytes += self.scratch.len() as u64;
        }
        start_slot
    }
    /// Writes `items` at element index `offset`, extending the live length to
    /// cover the range if it ends past it. This is the ARENA write path
    /// (ARCHITECTURE.md, Stage 1): ranges are placed by an external allocator
    /// and stay resident across frames, so there is no per-frame cursor -
    /// `clear`/`skip` are never called on an arena buffer and `live_len` is its
    /// high-water mark. Growth recreates the GPU buffer and rewrites the whole
    /// mirror, so every resident range survives a regrow at the same offsets
    /// (the arena invariant: blocks never move while live).
    pub fn write_at(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        offset: usize,
        items: &[T],
    ) where
        T: ShaderType + ShaderSize + WriteInto + Clone + Default,
    {
        if items.is_empty() {
            return;
        }
        let end = offset + items.len();
        let item_size = T::SHADER_SIZE.get() as usize;
        // Capacity check BEFORE any mutation (see `push`). Refusing here keeps
        // `live_len` below the range, so the arena block is never referenced as
        // live and no stale bytes are presented as geometry.
        if (end * item_size) as u64 > self.max_bytes {
            self.dropped_items += items.len() as u64;
            return;
        }
        if self.buffer_vec.len() < end {
            self.buffer_vec.resize(end, T::default());
        }
        self.buffer_vec[offset..end].clone_from_slice(items);
        self.live_len = self.live_len.max(end);

        let required_size = self.live_len * item_size;
        if self.buffer_wgpu.size() < required_size as u64 {
            let new_size = self.grown_size(required_size as u64, item_size as u64);
            self.buffer_wgpu = create_wgpu_buffer(device, self.label, new_size, self.usage);
            self.generation += 1;
            self.rewrite_all(queue);
        } else {
            let total_write = items.len() * item_size;
            self.scratch.clear();
            self.scratch.resize(total_write, 0);
            for (i, item) in items.iter().enumerate() {
                let item_offset = i * item_size;
                let slice = &mut self.scratch[item_offset..item_offset + item_size];
                let mut writer = encase::StorageBuffer::new(slice);
                writer
                    .write(item)
                    .expect("Failed to write to storage buffer");
            }
            queue.write_buffer(
                &self.buffer_wgpu,
                (offset * item_size) as u64,
                &self.scratch,
            );
            self.written_bytes += self.scratch.len() as u64;
        }
    }

    /// Rewinds the live length to 0 WITHOUT dropping the CPU mirror or GPU data,
    /// so next frame's writes overwrite in place and unwritten slots keep their
    /// previous contents (the basis for skipping unchanged writes).
    pub fn clear(&mut self) {
        self.live_len = 0;
    }

    /// Appends `items` at the live cursor, uploading only when the bytes already
    /// there differ. Returns whether an upload happened.
    ///
    /// This is the per-frame list path, and the decision is CONTENT-based on
    /// purpose. The buffer is one flat arena whose ranges are handed out by a
    /// per-frame cursor, so which producer owns a given range changes with the
    /// frame's composition - no key a caller can hold expresses "these bytes are
    /// still mine". [`holds_at`] answers exactly that question from the data, and
    /// what the skip saves is the GPU upload, not the CPU rebuild.
    pub fn write_or_skip(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, items: &[T]) -> bool
    where
        T: ShaderType + ShaderSize + WriteInto + Clone + PartialEq,
    {
        if holds_at(&self.buffer_vec, self.live_len, items) {
            self.live_len += items.len();
            return false;
        }
        let _ = self.push_bulk(device, queue, items);
        true
    }

    /// Lowers the binding ceiling so a test can drive the drop path without
    /// allocating the device's real 128 MiB limit.
    #[cfg(test)]
    pub fn set_max_bytes_for_test(&mut self, bytes: u64) {
        self.max_bytes = bytes & !3;
    }
}

/// Whether `mirror` already holds `items` at element index `start`.
///
/// The whole range has to be present and equal: a range that runs past what was
/// ever written is NOT a match, and neither is one that differs in a single
/// element. Callers use this to decide whether an upload is needed, so a false
/// positive would present another producer's bytes as their own.
fn holds_at<T: PartialEq>(mirror: &[T], start: usize, items: &[T]) -> bool {
    mirror.get(start..start + items.len()) == Some(items)
}

fn create_wgpu_buffer(
    device: &wgpu::Device,
    label: Option<&str>,
    size: wgpu::BufferAddress,
    usage: wgpu::BufferUsages,
) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label,
        size,
        usage,
        mapped_at_creation: false,
    })
}

/// Allocation size for `required` bytes: 1.5x growth, 4-aligned, at least
/// `BUFFER_MIN_ITEMS`, never past `max_bytes`. Callers must have already
/// established `required <= max_bytes`.
///
/// Invariant: capacity is clamped to the device storage-binding limit, so a
/// request past the limit yields the largest legal size instead of an
/// allocation wgpu rejects. A free function so that decision is testable
/// without a device.
fn grown_size(required: u64, item_size: u64, max_bytes: u64) -> u64 {
    let want = (((required as f32 * BUFFER_GROWTH_FACTOR) as u64)
        .max(BUFFER_MIN_ITEMS as u64 * item_size)
        + 3)
        & !3;
    want.min(max_bytes).max(required)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ITEM: u64 = 64; // GpuSegment
    const MAX: u64 = 64 * ITEM; // a deliberately tiny ceiling

    /// The clamp in [`grown_size`]: growth must never request more than the
    /// device's storage-binding limit, and must still cover `required`.
    #[test]
    fn growth_never_exceeds_the_binding_ceiling() {
        for n in 1..=64u64 {
            let size = grown_size(n * ITEM, ITEM, MAX);
            assert!(size <= MAX, "{n} items -> {size} B, past the {MAX} B cap");
            assert!(
                size >= n * ITEM,
                "{n} items -> {size} B, too small to hold them"
            );
            assert_eq!(size % 4, 0, "{n} items -> {size} B is not 4-aligned");
        }
    }

    /// Below the cap the 1.5x growth must still apply, or every push reallocates.
    #[test]
    fn growth_is_still_geometric_below_the_ceiling() {
        let required = 20 * ITEM;
        let size = grown_size(required, ITEM, u64::MAX);
        assert!(
            size >= required * 3 / 2,
            "expected ~1.5x of {required}, got {size}"
        );
    }

    /// A request that exactly fills the ceiling is representable and must not
    /// be rounded up past it.
    #[test]
    fn an_exactly_full_request_clamps_to_the_ceiling() {
        assert_eq!(grown_size(MAX, ITEM, MAX), MAX);
    }

    /// The reuse decision, isolated from any frame or GPU. `write_or_skip` skips
    /// the upload exactly when this says the range is already there, so a false
    /// positive would hand a producer another producer's bytes.
    mod holds_at {
        use super::super::holds_at;

        #[test]
        fn an_identical_range_matches() {
            assert!(holds_at(&[7, 8, 9, 10], 1, &[8, 9]));
        }

        #[test]
        fn one_differing_element_does_not_match() {
            assert!(!holds_at(&[7, 8, 9, 10], 1, &[8, 0]));
        }

        /// The shape the scatter lists fail in: same length, same tail, and only
        /// the leading draw-slot prefix belongs to somebody else.
        #[test]
        fn a_foreign_draw_slot_prefix_does_not_match() {
            let mirror = [6, 20, 6, 21, 6, 22];
            assert!(holds_at(&mirror, 0, &[6, 20, 6, 21, 6, 22]));
            assert!(!holds_at(&mirror, 0, &[7, 20, 7, 21, 7, 22]));
        }

        /// A range that runs past what was ever written is not a match, however
        /// far the prefix agrees - the tail would be uninitialized on the GPU.
        #[test]
        fn a_range_past_the_written_end_does_not_match() {
            assert!(!holds_at(&[7, 8], 1, &[8, 9]));
            assert!(!holds_at(&[7, 8], 3, &[8]));
        }

        /// Nothing to write is trivially already there, at any reachable cursor:
        /// `write_or_skip` must not upload and must not move the cursor.
        #[test]
        fn an_empty_range_matches_at_any_written_cursor() {
            assert!(holds_at::<u32>(&[7, 8], 0, &[]));
            assert!(holds_at::<u32>(&[7, 8], 2, &[]));
            assert!(holds_at::<u32>(&[], 0, &[]));
        }
    }
}

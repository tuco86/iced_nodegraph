//! Coarse-slot overflow telemetry (plan/exact-slot-allocation.md, option 3).
//!
//! The scatter cull appends (segment, entry) pairs into fixed-capacity coarse
//! tiles; past the usable cap the sort kernel drops pairs FIRST-COME, which is
//! only acceptable while it never actually happens in real scenes. This probe
//! makes overflow observable at zero steady-state cost: the per-tile demand
//! counters keep counting past the cap (true demand), so after each cull
//! dispatch they are copied into a small MAP_READ staging buffer and mapped
//! asynchronously. A later frame's `trim` harvests the completed readback
//! without ever blocking, scans the counts on the CPU (a few KB), and surfaces
//! the maximum per-tile demand plus the number of overflowing tiles through
//! `SdfStats` - at least one frame delayed, never stalling the pipeline.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use iced_wgpu::wgpu::{
    Buffer, BufferDescriptor, BufferUsages, CommandEncoder, Device, MapMode, PollType,
};

// Lifecycle of the in-flight staging buffer. `RECORDED` -> `PENDING` happens
// exactly once in `map_pending` (CAS-guarded: a cull whose readback is still
// outstanding must NOT re-map the buffer); the `map_async` callback then
// resolves `PENDING` to `READY` or `FAILED`.
const RECORDED: u8 = 0;
const PENDING: u8 = 1;
const READY: u8 = 2;
const FAILED: u8 = 3;

struct InFlight {
    buffer: Buffer,
    /// Bytes copied for this sample.
    used: u64,
    state: Arc<AtomicU8>,
}

/// One-deep asynchronous readback of a GPU counter buffer. At most one readback
/// is outstanding; a frame that finds one still in flight simply skips its
/// sample (telemetry may sample, it must never queue up or stall).
struct Readback {
    /// Staging buffer available for the next sample (replaced when undersized).
    idle: Option<Buffer>,
    in_flight: Option<InFlight>,
}

impl Readback {
    const fn new() -> Self {
        Self {
            idle: None,
            in_flight: None,
        }
    }

    /// Records a copy of the live prefix of `src` into a staging buffer. Call
    /// while recording the encoder, after the pass that writes `src`.
    fn record(
        &mut self,
        device: &Device,
        encoder: &mut CommandEncoder,
        src: &Buffer,
        used: u64,
        label: &'static str,
    ) {
        if self.in_flight.is_some() || used == 0 {
            return;
        }
        let buffer = match self.idle.take() {
            Some(b) if b.size() >= used => b,
            _ => device.create_buffer(&BufferDescriptor {
                label: Some(label),
                size: src.size(),
                usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
                mapped_at_creation: false,
            }),
        };
        encoder.copy_buffer_to_buffer(src, 0, &buffer, 0, used);
        self.in_flight = Some(InFlight {
            buffer,
            used,
            state: Arc::new(AtomicU8::new(RECORDED)),
        });
    }

    /// Starts the asynchronous map of the copy recorded by [`Readback::record`].
    /// Call after the recording encoder has been submitted. Maps each recorded
    /// copy exactly once: a call that finds the readback already mapped (a
    /// later frame while it is still in flight) is a no-op.
    fn map_pending(&self) {
        if let Some(f) = &self.in_flight
            && f.state
                .compare_exchange(RECORDED, PENDING, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
        {
            let state = Arc::clone(&f.state);
            f.buffer.slice(..f.used).map_async(MapMode::Read, move |r| {
                state.store(if r.is_ok() { READY } else { FAILED }, Ordering::Release);
            });
        }
    }

    /// Harvests a completed readback through `reduce`, if any. Never blocks:
    /// pumps the device with a non-waiting poll only while a readback is
    /// outstanding, and returns `None` until the map callback has fired.
    fn harvest<R>(&mut self, device: &Device, reduce: impl FnOnce(&[u8]) -> R) -> Option<R> {
        self.in_flight.as_ref()?;
        let _ = device.poll(PollType::Poll);
        match self.in_flight.as_ref()?.state.load(Ordering::Acquire) {
            RECORDED | PENDING => None,
            FAILED => {
                // Map failed (device loss etc.): drop the buffer, re-arm.
                self.in_flight = None;
                None
            }
            _ => {
                let f = self.in_flight.take().expect("state checked above");
                let report = {
                    let data = f.buffer.slice(..f.used).get_mapped_range();
                    reduce(&data)
                };
                f.buffer.unmap();
                self.idle = Some(f.buffer);
                Some(report)
            }
        }
    }
}

/// The pipeline's telemetry readbacks: the always-on coarse demand sample and
/// the opt-in per-fine-tile slot sample (see `crate::set_index_probe`). The two
/// are independent one-deep slots, so an outstanding fine readback never
/// suppresses a coarse sample.
pub struct OverflowProbe {
    coarse: Readback,
    fine: Readback,
}

/// A completed demand readback.
pub struct DemandReport {
    /// Highest per-tile pair demand across all coarse tiles.
    pub demand_max: u32,
    /// Tiles whose demand exceeded the usable slot cap (pairs were dropped).
    pub overflow_tiles: u32,
    /// Sum of per-tile demand clamped to the slot cap: the number of
    /// (segment, entry) pairs the sort kernel actually moves.
    pub demand_sum: u64,
}

/// A completed per-fine-tile slot readback. Each `fine_counts` word packs the
/// live slot count in its low 16 bits and the dropped-candidate count in its
/// high 16 bits (see `FINE_COUNT_MASK` in `shader.wgsl`).
#[derive(Clone, Copy, Default)]
pub struct FineReport {
    /// Sum of per-fine-tile referenced slot counts.
    pub slot_sum: u64,
    /// Highest per-fine-tile slot count (against `MAX_FINE_SLOTS` = 128).
    pub slot_max: u32,
    /// Fine tiles with at least one slot.
    pub live_tiles: u32,
    /// Fine tiles that dropped at least one candidate because they were full.
    /// Nonzero means some tile's slot list is INCOMPLETE.
    pub evicted_tiles: u32,
    /// Total candidates dropped across all full tiles.
    pub evicted_slots: u64,
}

impl OverflowProbe {
    pub const fn new() -> Self {
        Self {
            coarse: Readback::new(),
            fine: Readback::new(),
        }
    }

    /// Records a copy of the live prefix of the coarse demand counters. Call
    /// while recording the cull encoder, after the compute pass.
    pub fn record_coarse(
        &mut self,
        device: &Device,
        encoder: &mut CommandEncoder,
        counts: &Buffer,
        used: u64,
    ) {
        self.coarse
            .record(device, encoder, counts, used, "sdf_coarse_demand_readback");
    }

    /// Records a copy of the live prefix of the fine-tile slot counters. Call
    /// while recording the cull encoder, after the fine sort pass.
    pub fn record_fine(
        &mut self,
        device: &Device,
        encoder: &mut CommandEncoder,
        counts: &Buffer,
        used: u64,
    ) {
        self.fine
            .record(device, encoder, counts, used, "sdf_fine_slots_readback");
    }

    /// Starts the asynchronous map of both recorded copies. Call after the
    /// recording encoder has been submitted.
    pub fn map_pending(&self) {
        self.coarse.map_pending();
        self.fine.map_pending();
    }

    /// Harvests a completed coarse readback, if any. `usable_cap` is the
    /// per-tile slot count past which the sort kernel drops pairs; `slot_cap`
    /// is the hard per-tile capacity that clamps `demand_sum`.
    pub fn harvest_coarse(
        &mut self,
        device: &Device,
        usable_cap: u32,
        slot_cap: u32,
    ) -> Option<DemandReport> {
        self.coarse.harvest(device, |data| {
            let mut demand_max = 0u32;
            let mut overflow_tiles = 0u32;
            let mut demand_sum = 0u64;
            for chunk in data.chunks_exact(4) {
                let count = u32::from_le_bytes(chunk.try_into().expect("4-byte chunk"));
                demand_max = demand_max.max(count);
                overflow_tiles += u32::from(count > usable_cap);
                demand_sum += u64::from(count.min(slot_cap));
            }
            DemandReport {
                demand_max,
                overflow_tiles,
                demand_sum,
            }
        })
    }

    /// Harvests a completed fine-tile slot readback, if any.
    pub fn harvest_fine(&mut self, device: &Device) -> Option<FineReport> {
        self.fine.harvest(device, |data| {
            let mut r = FineReport::default();
            for chunk in data.chunks_exact(4) {
                let word = u32::from_le_bytes(chunk.try_into().expect("4-byte chunk"));
                let count = word & 0xFFFF;
                let evicted = word >> 16;
                r.slot_sum += u64::from(count);
                r.slot_max = r.slot_max.max(count);
                r.live_tiles += u32::from(count > 0);
                r.evicted_tiles += u32::from(evicted > 0);
                r.evicted_slots += u64::from(evicted);
            }
            r
        })
    }
}

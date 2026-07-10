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

use iced::wgpu::{
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
    /// Bytes copied for this cull (live coarse tiles * 4).
    used: u64,
    state: Arc<AtomicU8>,
}

/// One-deep asynchronous readback of the coarse demand counters. At most one
/// readback is outstanding; a cull that finds one still in flight simply skips
/// its sample (telemetry may sample, it must never queue up or stall).
pub struct OverflowProbe {
    /// Staging buffer available for the next cull (replaced when undersized).
    idle: Option<Buffer>,
    in_flight: Option<InFlight>,
}

/// A completed demand readback.
pub struct DemandReport {
    /// Highest per-tile pair demand across all coarse tiles.
    pub demand_max: u32,
    /// Tiles whose demand exceeded the usable slot cap (pairs were dropped).
    pub overflow_tiles: u32,
}

impl OverflowProbe {
    pub const fn new() -> Self {
        Self {
            idle: None,
            in_flight: None,
        }
    }

    /// Records a copy of the live prefix of the demand counters into a staging
    /// buffer. Call while recording the cull encoder, after the compute pass.
    pub fn record_copy(
        &mut self,
        device: &Device,
        encoder: &mut CommandEncoder,
        counts: &Buffer,
        used: u64,
    ) {
        if self.in_flight.is_some() || used == 0 {
            return;
        }
        let buffer = match self.idle.take() {
            Some(b) if b.size() >= used => b,
            _ => device.create_buffer(&BufferDescriptor {
                label: Some("sdf_coarse_demand_readback"),
                size: counts.size(),
                usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
                mapped_at_creation: false,
            }),
        };
        encoder.copy_buffer_to_buffer(counts, 0, &buffer, 0, used);
        self.in_flight = Some(InFlight {
            buffer,
            used,
            state: Arc::new(AtomicU8::new(RECORDED)),
        });
    }

    /// Starts the asynchronous map of the copy recorded by [`record_copy`].
    /// Call after the recording encoder has been submitted. Maps each recorded
    /// copy exactly once: a call that finds the readback already mapped (a
    /// later cull while it is still in flight) is a no-op.
    pub fn map_pending(&self) {
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

    /// Harvests a completed readback, if any. Never blocks: pumps the device
    /// with a non-waiting poll only while a readback is outstanding, and
    /// returns `None` until the map callback has fired. `usable_cap` is the
    /// per-tile slot count past which the sort kernel drops pairs.
    pub fn harvest(&mut self, device: &Device, usable_cap: u32) -> Option<DemandReport> {
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
                    let mut demand_max = 0u32;
                    let mut overflow_tiles = 0u32;
                    for chunk in data.chunks_exact(4) {
                        let count = u32::from_le_bytes(chunk.try_into().expect("4-byte chunk"));
                        demand_max = demand_max.max(count);
                        overflow_tiles += u32::from(count > usable_cap);
                    }
                    DemandReport {
                        demand_max,
                        overflow_tiles,
                    }
                };
                f.buffer.unmap();
                self.idle = Some(f.buffer);
                Some(report)
            }
        }
    }
}

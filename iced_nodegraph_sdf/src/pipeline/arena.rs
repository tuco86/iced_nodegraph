//! CPU-side range allocator for the persistent geometry arenas
//! (plan/arena-residency.md).
//!
//! Each of the segment/entry/style buffers is managed as an arena: a block is
//! allocated once, NEVER moves while live, and is returned to a free list on
//! eviction. This is what makes geometry reuse order-independent - a
//! primitive's resident ranges stay valid regardless of how the draw order
//! shuffles around it. The allocator itself is deliberately boring: a sorted,
//! coalesced free list with first-fit allocation and a bump high-water mark.
//! Fragmentation is not solved here; the pipeline resets the whole residency
//! state (compaction) when the high-water mark runs too far ahead of the live
//! count.

/// Range allocator over `u32` element indices. Allocation order: first-fit
/// from the free list, else bump the high-water mark (the backing GPU buffer
/// grows lazily on write).
#[derive(Default)]
pub(crate) struct ArenaAlloc {
    /// Disjoint free ranges `(start, len)` below `high_water`, sorted by
    /// `start`, adjacent ranges coalesced on `free`.
    free: Vec<(u32, u32)>,
    /// One past the highest element ever allocated; the backing buffer's
    /// required length.
    high_water: u32,
    /// Live (allocated) element count; `high_water - live` = free + never-used
    /// headroom below the mark. Drives the compaction heuristic.
    live: u32,
}

impl ArenaAlloc {
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocates `n` contiguous elements and returns the range start. `n == 0`
    /// returns 0 without touching any state (a valid empty range).
    pub fn alloc(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        self.live += n;
        // First-fit: smallest-index range that holds `n`.
        if let Some(i) = self.free.iter().position(|&(_, len)| len >= n) {
            let (start, len) = self.free[i];
            if len == n {
                self.free.remove(i);
            } else {
                self.free[i] = (start + n, len - n);
            }
            return start;
        }
        let start = self.high_water;
        self.high_water += n;
        start
    }

    /// Returns `[start, start + n)` to the free list, coalescing with adjacent
    /// free ranges. `n == 0` is a no-op. The range MUST be a live allocation
    /// (or a subrange of one); freeing twice corrupts the allocator.
    pub fn free(&mut self, start: u32, n: u32) {
        if n == 0 {
            return;
        }
        debug_assert!(start + n <= self.high_water, "free past high water");
        self.live -= n;
        // Insertion point by start; neighbours are the only coalesce candidates.
        let i = self.free.partition_point(|&(s, _)| s < start);
        debug_assert!(
            self.free.get(i).is_none_or(|&(s, _)| start + n <= s)
                && (i == 0 || {
                    let (ps, pl) = self.free[i - 1];
                    ps + pl <= start
                }),
            "double free or overlap at {start}+{n}"
        );
        let merges_prev = i > 0 && {
            let (ps, pl) = self.free[i - 1];
            ps + pl == start
        };
        let merges_next = self.free.get(i).is_some_and(|&(s, _)| start + n == s);
        match (merges_prev, merges_next) {
            (true, true) => {
                let (_, nl) = self.free.remove(i);
                self.free[i - 1].1 += n + nl;
            }
            (true, false) => self.free[i - 1].1 += n,
            (false, true) => {
                self.free[i].0 = start;
                self.free[i].1 += n;
            }
            (false, false) => self.free.insert(i, (start, n)),
        }
    }

    /// Drops every allocation and the high-water mark: the compaction reset.
    pub fn clear(&mut self) {
        self.free.clear();
        self.high_water = 0;
        self.live = 0;
    }

    pub fn live(&self) -> u32 {
        self.live
    }

    pub fn high_water(&self) -> u32 {
        self.high_water
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bump_allocation_is_contiguous() {
        let mut a = ArenaAlloc::new();
        assert_eq!(a.alloc(4), 0);
        assert_eq!(a.alloc(2), 4);
        assert_eq!(a.high_water(), 6);
        assert_eq!(a.live(), 6);
    }

    #[test]
    fn zero_sized_ops_are_noops() {
        let mut a = ArenaAlloc::new();
        assert_eq!(a.alloc(0), 0);
        a.free(0, 0);
        assert_eq!(a.high_water(), 0);
        assert_eq!(a.live(), 0);
    }

    #[test]
    fn freed_range_is_reused_first_fit() {
        let mut a = ArenaAlloc::new();
        let x = a.alloc(4);
        let y = a.alloc(4);
        a.free(x, 4);
        // Exact fit takes the freed range, not the high-water mark.
        assert_eq!(a.alloc(4), x);
        // Next allocation must not overlap y.
        assert_eq!(a.alloc(1), 8);
        let _ = y;
    }

    #[test]
    fn partial_fit_splits_range() {
        let mut a = ArenaAlloc::new();
        let x = a.alloc(8);
        let _y = a.alloc(1);
        a.free(x, 8);
        assert_eq!(a.alloc(3), 0);
        assert_eq!(a.alloc(5), 3);
        assert_eq!(a.high_water(), 9);
    }

    #[test]
    fn adjacent_frees_coalesce() {
        let mut a = ArenaAlloc::new();
        let x = a.alloc(2);
        let y = a.alloc(2);
        let z = a.alloc(2);
        let _guard = a.alloc(1);
        // Free out of order: [y], then [x] (merges before), then [z] (bridges).
        a.free(y, 2);
        a.free(x, 2);
        a.free(z, 2);
        // One coalesced 6-element range serves a 6-element allocation.
        assert_eq!(a.alloc(6), 0);
        assert_eq!(a.high_water(), 7);
    }

    #[test]
    fn live_tracks_alloc_and_free() {
        let mut a = ArenaAlloc::new();
        let x = a.alloc(5);
        assert_eq!(a.live(), 5);
        a.free(x, 5);
        assert_eq!(a.live(), 0);
        assert_eq!(a.high_water(), 5);
        a.clear();
        assert_eq!(a.high_water(), 0);
    }
}

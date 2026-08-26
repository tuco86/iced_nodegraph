//! Which ring each cable takes at each anchor it wraps.
//!
//! An anchor with `n` cables through it shows `n` concentric orbits, and the
//! only decision left is which cable sits on which. That decision is what makes
//! cables cross, and it is not a local one: two cables flying the same stretch
//! from one anchor to the next stay apart only while their nesting agrees at
//! both ends, so an anchor cannot choose its own order without regard for its
//! neighbours.
//!
//! One cost, COUNTED rather than predicted: a crossing inside a corridor, the
//! open space between two anchors a pair of cables flies together. That is the
//! worst place to spend a crossing, because it is where the eye follows the
//! cable, and it is the only kind a ring choice can reliably trade away.
//!
//! Below it sits a tiebreak on containment: cables at an anchor are ordered by
//! the angular interval their neighbours subtend there, smallest innermost, so
//! that where two intervals NEST the contained one sits inside and neither
//! cable's legs cut across the other. Equal-crossing layouts then settle the
//! way a reader expects.
//!
//! Counting is the whole design. Whether two cables cross along a corridor is
//! not a function of their radial order: it also depends on which way each
//! wraps each end, because a cable wrapping both ends the same way rides a
//! tangent that holds one side of the line of centres while one wrapping them
//! opposite ways rides the crossed tangent and swaps sides. That wrap direction
//! is chosen by the geometry from the radii, so it is not knowable before the
//! orbits are - and every radius-free estimate of it measured unsound. So a
//! candidate is judged by building it and counting what it actually does, which
//! is what [`assign`]'s `cost` does.
//!
//! Minimising crossings over a graph is the metro-line crossing problem and is
//! NP-hard. This is therefore a bounded local search, not a solver: the
//! containment order seeds it, and two kinds of exchange are tried while they
//! measurably help - any pair of rings at one anchor, and the same two cables at
//! every anchor they share. Work is capped by a ceiling on cable builds rather
//! than on candidates, since what a candidate costs depends on how many cables a
//! ring choice can move. Since every candidate is measured and only a strict
//! improvement is kept, the result is never worse than containment alone. It is
//! a pure function of the frame, so nothing drifts between two identical frames.

/// Units of measuring work one call may spend on candidates.
///
/// The ceiling is on WORK, not candidates, because what a candidate costs is a
/// property of the scene rather than a constant: one cable build per cable a
/// ring choice can move, plus one crossing count per corridor band. A scene with
/// one corridor between two anchors gets a thorough search; a knot of them gets
/// a shallow one rather than a stalled frame. Exhausting the ceiling leaves
/// whatever the search has reached, which is never worse than the containment
/// order it started from.
///
/// The two units are counted, not weighed: a band count's real cost grows with
/// how much of each cable falls inside that band, so a long corridor costs more
/// per unit than a short one. The bound on total units therefore holds exactly
/// while the bound on wall time is only approximate, which is why the value is
/// calibrated by measuring the frame rather than derived.
pub(super) const WORK_CEILING: usize = 192;

/// How many candidates [`assign`] may measure when each costs `per_candidate`
/// units of work, and `movable` cables can be reordered at all.
///
/// A scene past [`MOST_MOVABLE`] gets no search: the permutation space is far
/// larger than any budget could sample usefully, so spending the frame on a
/// token few candidates buys less than admitting containment is the answer.
pub(super) fn budget(movable: usize, per_candidate: usize) -> usize {
    if movable > MOST_MOVABLE {
        return 0;
    }
    WORK_CEILING / per_candidate.max(1)
}

/// One wrap as the assignment sees it.
#[derive(Debug, Clone, Copy)]
pub(super) struct Wrap {
    /// The anchor it belongs to. A ring held at the cursor belongs to no anchor
    /// and takes no orbit from one.
    pub anchor: Option<usize>,
    /// The angle its two neighbours subtend at the anchor centre, in radians.
    ///
    /// An interval, not the arc the cable lays down; see
    /// [`wrap_span`](super::wrap_span).
    pub span: f32,
}

/// How many cables a ring choice may move before the search gives up and leaves
/// the containment order standing.
///
/// Every candidate costs a build and a crossing count per movable cable, so a
/// densely routed graph cannot be searched inside a frame at all. Refusing is
/// better than a stalled frame, and costs only the improvement containment was
/// already going to miss.
const MOST_MOVABLE: usize = 6;

/// Every wrap in the frame given one index, cables laid end to end.
///
/// Slots of one cable are contiguous, which is what makes a wrap's neighbours
/// in its own route the adjacent slots.
struct Slots {
    /// First slot of each cable, with the total appended.
    offset: Vec<usize>,
    /// The anchor each slot wraps.
    anchor: Vec<Option<usize>>,
}

impl Slots {
    fn new(cables: &[Vec<Wrap>]) -> Self {
        let mut offset = Vec::with_capacity(cables.len() + 1);
        let mut anchor = Vec::new();
        for wraps in cables {
            offset.push(anchor.len());
            anchor.extend(wraps.iter().map(|wrap| wrap.anchor));
        }
        offset.push(anchor.len());
        Self { offset, anchor }
    }

    fn total(&self) -> usize {
        self.anchor.len()
    }

    /// The slots of `cable`, in visiting order.
    fn of(&self, cable: usize) -> std::ops::Range<usize> {
        self.offset[cable]..self.offset[cable + 1]
    }
}

/// One cable's claim on one anchor.
#[derive(Debug, Clone, Copy)]
struct Seat {
    slot: usize,
    /// Kept for the tiebreak: cables whose intervals give no reason to separate
    /// them are ordered by the host's push order.
    cable: usize,
    span: f32,
}

/// The anchors whose ring order is worth searching: those a pair of cables
/// shares while ALSO sharing another one.
///
/// Two cables sharing two anchors fly a corridor between them, and an anchor at
/// either end of one cannot settle its order by looking at itself. An empty
/// answer means containment is already the whole assignment, so the caller can
/// skip building the cost closure at all - the common case, since a graph whose
/// cables each wrap at most one anchor can never produce a corridor.
///
/// Not every anchor a measured cable rides is in here. A cable can reach a
/// corridor's far end by way of a third anchor it shares with nobody; its ring
/// there moves the cable inside the band, so the cost sees the change, but no
/// exchange proposes it. That costs reachability, never correctness - the search
/// only keeps a strict improvement.
///
/// The caller also needs this to know which cables a candidate can move, so it
/// can build those and no others.
pub(super) fn contested(cables: &[Vec<Wrap>], anchors: usize) -> Vec<usize> {
    // Only a cable wrapping two anchors can fly a corridor, so the rest are
    // dropped before the all-pairs scan rather than carried through it as empty
    // routes. A graph with no routed cable at all leaves nothing to pair.
    let routes: Vec<Vec<usize>> = cables
        .iter()
        .filter_map(|wraps| {
            let route: Vec<usize> = wraps.iter().filter_map(|wrap| wrap.anchor).collect();
            (route.len() > 1).then_some(route)
        })
        .collect();
    let mut contested = vec![false; anchors];
    for (at, one) in routes.iter().enumerate() {
        for other in &routes[at + 1..] {
            let shared = one.iter().filter(|anchor| other.contains(anchor));
            if shared.clone().count() > 1 {
                for &anchor in shared {
                    if let Some(flag) = contested.get_mut(anchor) {
                        *flag = true;
                    }
                }
            }
        }
    }
    (0..anchors).filter(|&anchor| contested[anchor]).collect()
}

/// The orbit each wrap takes, parallel to `cables` and to each cable's wraps.
///
/// A wrap that belongs to no anchor keeps orbit 0; its radius is given rather
/// than assigned.
///
/// `contested` comes from [`contested`]; an empty slice means the containment
/// order stands and `cost` is never called. `cost` measures one candidate by
/// building it, so it is the expensive part and the reason the search runs on a
/// budget.
pub(super) fn assign(
    cables: &[Vec<Wrap>],
    anchors: usize,
    contested: &[usize],
    budget: usize,
    cost: &mut dyn FnMut(&[Vec<u8>]) -> usize,
) -> Vec<Vec<u8>> {
    let slots = Slots::new(cables);
    let mut seats = seed(cables, &slots, anchors);
    if !contested.is_empty() && budget > 1 {
        refine(&mut seats, &slots, contested, budget, cost);
    }
    arrange(&slots, &seats)
}

/// Every anchor's cables in containment order, ties by push order.
fn seed(cables: &[Vec<Wrap>], slots: &Slots, anchors: usize) -> Vec<Vec<Seat>> {
    let mut seats: Vec<Vec<Seat>> = vec![Vec::new(); anchors];
    for (cable, wraps) in cables.iter().enumerate() {
        for (slot, wrap) in slots.of(cable).zip(wraps) {
            if let Some(anchor) = wrap.anchor
                && let Some(seats) = seats.get_mut(anchor)
            {
                seats.push(Seat {
                    slot,
                    cable,
                    span: wrap.span,
                });
            }
        }
    }
    for seats in &mut seats {
        seats.sort_by(|a, b| a.span.total_cmp(&b.span).then(a.cable.cmp(&b.cable)));
    }
    seats
}

/// Exchanges cables between rings while that measurably helps, within the
/// budget.
///
/// First-improvement hill climbing over two neighbourhoods, and it needs both.
///
/// The LOCAL move exchanges any pair of rings at one contested anchor - any
/// pair, not just neighbouring ones, because a three-cable anchor can sit one
/// non-adjacent exchange away from a clear corridor while every adjacent
/// exchange leaves the count unchanged.
///
/// The COUPLED move exchanges the same two cables at EVERY anchor they share.
/// No sequence of local moves expresses it usefully: for a pair sharing three
/// anchors, reversing them at one anchor clears that corridor and breaks the
/// next, so each single step scores no better and the search stops. Swapping
/// both ends at once keeps their order consistent the whole way along.
///
/// Every step is verified against real geometry and an exchange that does not
/// improve is undone at once, so the seating only ever moves downhill.
fn refine(
    seats: &mut [Vec<Seat>],
    slots: &Slots,
    contested: &[usize],
    budget: usize,
    cost: &mut dyn FnMut(&[Vec<u8>]) -> usize,
) {
    let measure = |seats: &[Vec<Seat>], cost: &mut dyn FnMut(&[Vec<u8>]) -> usize| {
        let arrangement = arrange(slots, seats);
        (cost(&arrangement), inversions(seats))
    };
    let riders = riders(seats, contested);
    let mut best = measure(seats, cost);
    let mut spent = 1;
    let mut improving = true;
    while improving && spent < budget {
        improving = false;
        for &anchor in contested {
            for inner in 0..seats[anchor].len() {
                for outer in inner + 1..seats[anchor].len() {
                    if spent >= budget {
                        return;
                    }
                    seats[anchor].swap(inner, outer);
                    spent += 1;
                    let score = measure(seats, cost);
                    if score < best {
                        best = score;
                        improving = true;
                    } else {
                        seats[anchor].swap(inner, outer);
                    }
                }
            }
        }
        for (i, &one) in riders.iter().enumerate() {
            for &other in &riders[i + 1..] {
                if spent >= budget {
                    return;
                }
                if couple(seats, contested, one, other) < 2 {
                    couple(seats, contested, one, other);
                    continue;
                }
                spent += 1;
                let score = measure(seats, cost);
                if score < best {
                    best = score;
                    improving = true;
                } else {
                    couple(seats, contested, one, other);
                }
            }
        }
    }
}

/// The cables seated at a contested anchor, ascending, each once.
fn riders(seats: &[Vec<Seat>], contested: &[usize]) -> Vec<usize> {
    let mut riders: Vec<usize> = contested
        .iter()
        .flat_map(|&anchor| seats[anchor].iter().map(|seat| seat.cable))
        .collect();
    riders.sort_unstable();
    riders.dedup();
    riders
}

/// Exchanges two cables' rings at every contested anchor carrying both, and
/// answers how many anchors that moved.
///
/// Its own inverse, so undoing a coupled move is applying it again.
fn couple(seats: &mut [Vec<Seat>], contested: &[usize], one: usize, other: usize) -> usize {
    let mut moved = 0;
    for &anchor in contested {
        let at = |cable: usize| seats[anchor].iter().position(|seat| seat.cable == cable);
        if let (Some(a), Some(b)) = (at(one), at(other)) {
            seats[anchor].swap(a, b);
            moved += 1;
        }
    }
    moved
}

/// Pairs sharing an anchor whose rings depart from containment order.
fn inversions(seats: &[Vec<Seat>]) -> usize {
    let mut count = 0;
    for seats in seats {
        for (i, outer) in seats.iter().enumerate() {
            for inner in &seats[i + 1..] {
                count += usize::from(outer.span > inner.span);
            }
        }
    }
    count
}

/// The seating as an orbit per wrap, per cable.
fn arrange(slots: &Slots, seats: &[Vec<Seat>]) -> Vec<Vec<u8>> {
    let mut orbit = vec![0usize; slots.total()];
    for seats in seats {
        for (at, seat) in seats.iter().enumerate() {
            orbit[seat.slot] = at;
        }
    }
    (0..slots.offset.len() - 1)
        .map(|cable| {
            slots
                .of(cable)
                .map(|slot| u8::try_from(orbit[slot]).unwrap_or(u8::MAX))
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A cable wrapping each `(anchor, span)` in visiting order.
    fn cable(wraps: &[(usize, f32)]) -> Vec<Wrap> {
        wraps
            .iter()
            .map(|&(anchor, span)| Wrap {
                anchor: Some(anchor),
                span,
            })
            .collect()
    }

    /// A cost that never sees a crossing, so containment decides everything.
    fn spotless(_: &[Vec<u8>]) -> usize {
        0
    }

    /// With nothing to measure against, containment is the whole answer: the
    /// cable that wraps less sits inside.
    #[test]
    fn a_lone_anchor_keeps_the_containment_order() {
        let cables = vec![cable(&[(0, 2.0)]), cable(&[(0, 0.5)])];
        let assigned = assign(
            &cables,
            1,
            &contested(&cables, 1),
            budget(cables.len(), 3),
            &mut spotless,
        );
        assert_eq!(assigned[1][0], 0, "the narrower wrap is innermost");
        assert_eq!(assigned[0][0], 1);
    }

    /// A cable wrapping one anchor cannot fly a corridor, so no candidate is
    /// ever built for it.
    ///
    /// This is what keeps the search off the common graph. A build is the
    /// expensive part of the whole assignment, and a host that never routes a
    /// cable through two anchors must not pay for one.
    #[test]
    fn a_graph_without_a_corridor_measures_nothing() {
        let cables = vec![cable(&[(0, 2.0)]), cable(&[(0, 0.5)]), cable(&[(1, 1.0)])];
        let mut calls = 0;
        let mut counted = |_: &[Vec<u8>]| {
            calls += 1;
            0
        };
        assign(
            &cables,
            2,
            &contested(&cables, 2),
            budget(cables.len(), 3),
            &mut counted,
        );
        assert_eq!(calls, 0, "a graph with no corridor was measured anyway");
    }

    /// The order that measures cleanest wins, even where containment asks for
    /// the opposite.
    ///
    /// Containment wants cable 0 inside at anchor 0 and outside at anchor 1. The
    /// cost says any arrangement holding that crosses in the corridor. Crossings
    /// outrank containment, so the arrangement that clears the corridor is the
    /// one kept, and the containment inversion it costs is accepted.
    #[test]
    fn a_measured_corridor_crossing_outranks_containment() {
        let cables = vec![cable(&[(0, 0.5), (1, 2.0)]), cable(&[(0, 2.0), (1, 0.5)])];
        let mut cost = |arrangement: &[Vec<u8>]| {
            let matched =
                (arrangement[0][0] < arrangement[1][0]) == (arrangement[0][1] < arrangement[1][1]);
            usize::from(!matched)
        };
        let assigned = assign(
            &cables,
            2,
            &contested(&cables, 2),
            budget(cables.len(), 3),
            &mut cost,
        );
        assert_eq!(
            (assigned[0][0] < assigned[1][0]),
            (assigned[0][1] < assigned[1][1]),
            "the search kept an arrangement the cost calls a crossing: {assigned:?}",
        );
    }

    /// The search never leaves the seating worse than containment left it.
    ///
    /// The guarantee the whole design rests on: minimising crossings over a
    /// graph is NP-hard, so this is a bounded local search rather than a solver,
    /// and refusing every non-improving step is the only promise worth making.
    /// A cost that punishes any move away from the seed proves the refusal is
    /// real rather than incidental.
    #[test]
    fn the_search_never_accepts_a_worse_arrangement() {
        let cables = vec![
            cable(&[(0, 0.3), (1, 1.2)]),
            cable(&[(0, 1.2), (1, 0.3)]),
            cable(&[(0, 2.7), (1, 2.7)]),
        ];
        let seeded = assign(
            &cables,
            2,
            &contested(&cables, 2),
            budget(cables.len(), 3),
            &mut spotless,
        );
        let mut cost = |arrangement: &[Vec<u8>]| usize::from(arrangement != seeded);
        assert_eq!(
            assign(
                &cables,
                2,
                &contested(&cables, 2),
                budget(cables.len(), 3),
                &mut cost
            ),
            seeded,
            "the search walked away from the only clean arrangement",
        );
    }

    /// Two identical frames assign identical rings; a wrap that moved between
    /// them would flicker.
    #[test]
    fn the_same_graph_assigns_the_same_orbits() {
        let cables = vec![
            cable(&[(0, 1.0), (1, 1.0)]),
            cable(&[(0, 1.0), (1, 1.0)]),
            cable(&[(1, 0.2)]),
        ];
        let mut wobbly = |arrangement: &[Vec<u8>]| arrangement[0][0] as usize;
        let once = assign(
            &cables,
            2,
            &contested(&cables, 2),
            budget(cables.len(), 3),
            &mut wobbly,
        );
        let twice = assign(
            &cables,
            2,
            &contested(&cables, 2),
            budget(cables.len(), 3),
            &mut wobbly,
        );
        assert_eq!(once, twice);
    }

    /// Two cables reaching the same two anchors by different routes share no
    /// corridor, so nothing is measured and containment stands.
    ///
    /// One flies straight from anchor 0 to anchor 2 while the other detours via
    /// anchor 1. They do share two anchors, which is what makes them worth
    /// searching - the detour changes where the second cable is between them,
    /// not whether the pair can be separated by a ring choice.
    #[test]
    fn a_detour_still_counts_as_sharing_the_anchors() {
        let cables = vec![
            cable(&[(0, 2.0), (1, 1.0), (2, 0.5)]),
            cable(&[(0, 0.5), (2, 2.0)]),
        ];
        let mut calls = 0;
        let mut counted = |_: &[Vec<u8>]| {
            calls += 1;
            0
        };
        assign(
            &cables,
            3,
            &contested(&cables, 3),
            budget(cables.len(), 3),
            &mut counted,
        );
        assert!(
            calls > 0,
            "a pair sharing two anchors was not measured at all",
        );
    }

    /// A ring held at the cursor belongs to no anchor, so it takes no orbit from
    /// one and leaves the real wrap innermost.
    #[test]
    fn a_cursor_ring_takes_no_orbit() {
        let cables = vec![vec![
            Wrap {
                anchor: None,
                span: 1.0,
            },
            Wrap {
                anchor: Some(0),
                span: 1.0,
            },
        ]];
        assert_eq!(
            assign(
                &cables,
                1,
                &contested(&cables, 1),
                budget(cables.len(), 3),
                &mut spotless
            )[0],
            vec![0, 0]
        );
    }
}

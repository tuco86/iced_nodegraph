//! Cable topology: the single walk from graph topology to the hop chain each
//! edge draws as. Pins resolve to [`Station`]s, a route resolves to anchor
//! indices in visiting order, and [`orbits::assign`] picks each wrap's ring;
//! drawing, cutting, hit-testing and hover all read the [`CableGeometry`] this
//! produces, so what a gesture aims at is what the frame put on screen.

use super::edge_path::{self, Border};
use super::orbits;
use super::{NodeGraph, PinRef};
use crate::ids::Ids;
use crate::node_pin::{PinDirection, PinSide};
use crate::style::{Catalog, EdgeCurve};

/// A pin resolved for this frame: where it is and which way its side faces.
///
/// The output of a caller's endpoint resolver. The widget's two halves work in
/// different coordinate spaces (layout-absolute for drawing, world for input),
/// and this is where they meet; `direction` is what orients a cable
/// output-first.
///
/// A [`PinSide::Row`] pin spans its node and offers a border on either side of
/// it, so its station arrives with both and [`Station::settle`] picks the one
/// this cable takes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Station {
    pub point: [f32; 2],
    pub side: Border,
    pub direction: Option<PinDirection>,
    /// The border still on offer, with its point, until `settle` chooses
    /// between the two. `None` for a pin that declares one side.
    across: Option<([f32; 2], Border)>,
}

impl Station {
    /// The station a pin offers a cable, from the `(start, end)` anchors its
    /// node border gives it and the side it declares.
    ///
    /// [`PinSide::Row`] is the one side with two anchors, and the only one whose
    /// station leaves the choice open: it spans the node, so a cable takes the
    /// border nearer its other end. Every other side collapses to its single
    /// anchor and the outward normal it names.
    pub fn for_pin(side: PinSide, anchors: ([f32; 2], [f32; 2]), direction: PinDirection) -> Self {
        let one_sided = |border| Self::at(anchors.0, border, Some(direction));
        match side {
            PinSide::Left => one_sided(Border::Left),
            PinSide::Right => one_sided(Border::Right),
            PinSide::Top => one_sided(Border::Top),
            PinSide::Bottom => one_sided(Border::Bottom),
            PinSide::Row => Self::row(anchors.0, anchors.1, Some(direction)),
        }
    }

    /// A pin on one side of its node: one anchor, one outward normal.
    pub fn at(point: [f32; 2], side: Border, direction: Option<PinDirection>) -> Self {
        Self {
            point,
            side,
            direction,
            across: None,
        }
    }

    /// A [`PinSide::Row`] pin, offering both vertical borders of its node.
    ///
    /// The left border stands in until [`Station::settle`] has seen the far
    /// end; every path that draws a cable goes through
    /// [`edge_hops`](NodeGraph::edge_hops), which settles both ends first.
    pub fn row(left: [f32; 2], right: [f32; 2], direction: Option<PinDirection>) -> Self {
        Self {
            point: left,
            side: Border::Left,
            direction,
            across: Some((right, Border::Right)),
        }
    }

    /// The point the far end of a cable measures ITS side against: the middle
    /// of the borders on offer.
    ///
    /// Two row pins facing each other would otherwise each need the other's
    /// choice to make their own. The midpoint is the one value neither has to
    /// have decided yet, and it is the pin's own position wherever there is
    /// nothing to choose.
    pub fn aim(&self) -> [f32; 2] {
        match self.across {
            Some((other, _)) => [
                0.5 * (self.point[0] + other[0]),
                0.5 * (self.point[1] + other[1]),
            ],
            None => self.point,
        }
    }

    /// Settles which border a row pin's cable leaves by, given where it runs
    /// from here.
    ///
    /// Both borders sit at the same height, so the vertical distance is common
    /// to them and the nearer one is whichever side of the node's centre line
    /// `toward` lies on. The choice therefore flips exactly once, as the far
    /// end crosses that line, which is what makes hysteresis unnecessary; a
    /// tie keeps the left border. A pin that declares one side has nothing to
    /// settle, and a station settled twice keeps its first choice.
    pub fn settle(&mut self, toward: [f32; 2]) {
        let Some((point, side)) = self.across.take() else {
            return;
        };
        if (toward[0] - point[0]).abs() < (toward[0] - self.point[0]).abs() {
            self.point = point;
            self.side = side;
        }
    }
}

/// One edge's topology lowered to the hop chain it draws as.
///
/// The single walk from graph topology to cable geometry: drawing, cutting,
/// hit-testing and hover all read this, so what a gesture aims at is what the
/// frame put on screen.
#[derive(Debug)]
pub(super) struct CableGeometry<'e, I: Ids> {
    /// Index into `NodeGraph::edges`.
    pub edge: usize,
    /// The stations to build, output pin first.
    pub hops: Vec<edge_path::Hop>,
    /// Hop index -> the `(anchor index, orbit)` that hop wraps, for every wrap
    /// hop in `hops`. A phantom wrap contributes no entry: it names no anchor
    /// the host owns.
    pub rings: Vec<(usize, (usize, u8))>,
    /// Both endpoint pins, oriented output -> input like `hops`.
    pub ends: (&'e PinRef<I>, &'e PinRef<I>),
}

/// A wrap inserted into one edge's route for the duration of a route drag, so
/// the previewed cable runs where the committed one will.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct RoutePhantom {
    /// The edge being re-routed, as an index into `NodeGraph::edges`.
    pub edge: usize,
    /// An anchor dropped from the preview because a detach was published for
    /// it. Held until the host applies that detach, so the cable does not snap
    /// back to the anchor for one frame.
    pub exclude: Option<usize>,
    pub kind: PhantomKind,
}

impl RoutePhantom {
    /// The route edit this preview stands for, as
    /// [`anchor_rings`](NodeGraph::anchor_rings) must count it.
    ///
    /// One derivation for both halves of the widget, so the orbit `draw`
    /// previews and the ring `update` measures against cannot disagree.
    pub fn pending(&self) -> PendingRoute {
        PendingRoute {
            edge: self.edge,
            attach: match self.kind {
                PhantomKind::Snap { anchor } => Some(anchor),
                // A ring at the cursor belongs to no anchor, so it takes no
                // orbit from one.
                PhantomKind::At { .. } => None,
            },
            detach: self.exclude,
        }
    }
}

/// A route drag's edit to the host's routes, before the host has applied it.
///
/// Folded into the occupancy the frame derives so a drag predicts the orbit the
/// round trip will produce rather than the one at the end of the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PendingRoute {
    /// The edge the drag holds, as an index into `NodeGraph::edges`.
    pub edge: usize,
    /// The anchor the drag has attached it to.
    pub attach: Option<usize>,
    /// The anchor the drag has pulled it off.
    pub detach: Option<usize>,
}

/// What the phantom wrap is laid tangent to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum PhantomKind {
    /// A ring at the cursor, unattached: the drag has not snapped to anything.
    At { center: [f32; 2], radius: f32 },
    /// The orbit offered by a snapped anchor. Inserted only when the host's own
    /// route does not already carry that anchor, so it self-corrects once the
    /// attach round-trips.
    Snap { anchor: usize },
}

/// One cable resolved as far as it can be before an orbit is chosen: its two
/// pins and the wraps in visiting order.
struct CablePlan<'e, I: Ids> {
    edge: usize,
    head: Station,
    tail: Station,
    ends: (&'e PinRef<I>, &'e PinRef<I>),
    wraps: Vec<WrapPlan>,
}

impl<I: Ids> CablePlan<'_, I> {
    /// The hop chain this cable draws as, given a ring per wrap.
    ///
    /// Called once per candidate arrangement during the orbit search and once
    /// more for the arrangement that wins, so the hop chain and the rings a
    /// candidate was judged on are the ones that ship - identical by
    /// construction, not by agreement between two walks.
    ///
    /// The PATH built from that chain can still differ on the draw side, which
    /// resolves this frame's `EdgeCurve` for the stroke while the search measures
    /// against the curve the last frame published. A host switching a curve
    /// therefore settles the ring assignment one frame later, and the first
    /// frame of all measures against the default.
    ///
    /// The second half is the ring each wrap hop landed on, by hop index, which
    /// only this walk knows: a wrap whose circle does not resolve contributes no
    /// hop and so shifts every index after it.
    fn chain(
        &self,
        orbits: &[u8],
        ring: &dyn Fn(usize, u8) -> Option<edge_path::Orbit>,
    ) -> (Vec<edge_path::Hop>, Vec<(usize, (usize, u8))>) {
        let mut hops = Vec::with_capacity(self.wraps.len() + 2);
        let mut rings = Vec::with_capacity(self.wraps.len());
        hops.push(edge_path::Hop::Pin {
            point: self.head.point,
            side: self.head.side,
        });
        for (at, wrap) in self.wraps.iter().enumerate() {
            let orbit = orbits.get(at).copied().unwrap_or(0);
            let circle = match (wrap.anchor, wrap.radius) {
                (_, Some(radius)) => Some(edge_path::Orbit {
                    center: wrap.center,
                    radius,
                }),
                (Some(anchor), None) => ring(anchor, orbit),
                (None, None) => None,
            };
            let Some(circle) = circle else { continue };
            if let Some(anchor) = wrap.anchor {
                rings.push((hops.len(), (anchor, orbit)));
            }
            hops.push(edge_path::Hop::Wrap { orbit: circle });
        }
        hops.push(edge_path::Hop::Pin {
            point: self.tail.point,
            side: self.tail.side,
        });
        (hops, rings)
    }

    /// The anchors both cables wrap, in the order this one reaches them.
    ///
    /// Two cables can only be moved relative to one another by a ring choice
    /// where they meet, so this is what decides whether a pair is worth
    /// measuring at all. Visiting order rather than index order, because
    /// CONSECUTIVE entries are the stretches the pair actually flies together:
    /// two anchors with a third shared one between them are not one corridor but
    /// two, and the band spanning them adds nothing the two halves do not
    /// already cover.
    fn shared_anchors(&self, other: &Self) -> Vec<usize> {
        self.wraps
            .iter()
            .filter_map(|wrap| wrap.anchor)
            .filter(|anchor| other.wraps.iter().any(|wrap| wrap.anchor == Some(*anchor)))
            .collect()
    }
}

/// One wrap before its radius is known.
struct WrapPlan {
    /// The anchor it belongs to, or `None` for a ring held at the cursor, which
    /// belongs to no anchor and so takes no orbit from one.
    anchor: Option<usize>,
    center: [f32; 2],
    /// Set only for a cursor ring, whose radius is given rather than assigned.
    radius: Option<f32>,
    /// The angular interval its neighbours subtend at the anchor, the key its
    /// orbit is assigned by.
    span: f32,
}

/// The angle between a wrap's two neighbouring stations, seen from the anchor
/// centre, folded into `[0, PI]`.
///
/// An interval, NOT the arc the cable lays down - the two are anti-correlated.
/// The tangents sit `acos(r / d)` off each neighbour's bearing, so with
/// `A = acos(r / d_prev) + acos(r / d_next)` the realized arc is
/// `folded(delta - A)`: the cable whose neighbours subtend the LEAST goes
/// furthest round the ring.
///
/// The interval is what orders two cables, and it is deliberately what is
/// measured here. Where two cables enter an anchor from the same side and leave
/// to the same side, their intervals NEST, and the contained one belongs inside:
/// seat it outside and its legs have to cut across the other cable twice. That
/// question is settled by the intervals alone, which is why this reads only the
/// centre and the two neighbours - and so is known before any radius is, which
/// is what lets it choose one.
pub(super) fn wrap_span(center: [f32; 2], prev: [f32; 2], next: [f32; 2]) -> f32 {
    let bearing = |p: [f32; 2]| (p[1] - center[1]).atan2(p[0] - center[0]);
    let delta = (bearing(next) - bearing(prev)).rem_euclid(std::f32::consts::TAU);
    delta.min(std::f32::consts::TAU - delta)
}

impl<'a, I: Ids, Message, Theme: Catalog, Renderer> NodeGraph<'a, I, Message, Theme, Renderer> {
    /// One edge's [`route`](super::Edge::route) as anchor indices: ids resolved
    /// through `anchor_lookup`, unknown ids dropped, repeats collapsed to
    /// their first occurrence.
    ///
    /// Order is the host's authoring order, which only matters as the
    /// tie-breaker when the run gives no ordering at all (see
    /// [`edge_hops`](Self::edge_hops)).
    pub(super) fn resolved_route(&self, edge: usize) -> Vec<usize> {
        let Some(edge) = self.edges.get(edge) else {
            return Vec::new();
        };
        let mut out: Vec<usize> = Vec::with_capacity(edge.route.len());
        for id in &edge.route {
            if let Some(index) = self.anchor_index(id)
                && !out.contains(&index)
            {
                out.push(index);
            }
        }
        out
    }

    /// How many rings each anchor shows: the number of edges routed through it,
    /// indexed by anchor index.
    ///
    /// A count, not an order. Which cable rides which ring is decided in
    /// [`orbits::assign`] from the angular intervals its neighbours subtend plus
    /// a measured crossing search; this answers only how many there are, which
    /// is what bounds an anchor's drawn extent and how far a route drag can
    /// reach it.
    ///
    /// `pending` folds in a route drag's edit before the host has applied it, so
    /// the ring the drag measures against is the one the frame draws rather than
    /// the one the graph carried before the gesture started.
    ///
    /// An anchor is capped at `u8::MAX + 1` rings, since an orbit index is a
    /// `u8`; the surplus is not counted and so not drawn.
    ///
    /// Counted off the ROUTES the host authored, not off the cables that get
    /// built: [`edge_hops`](Self::edge_hops) drops an edge whose endpoint pin
    /// this frame's content does not contain, and such an edge still counts a
    /// ring here. An anchor named only by dropped edges therefore draws a ring
    /// no cable rides, and a route drag reaches it one ring wide. The
    /// disagreement is only ever in that direction - the cables seated at an
    /// anchor are a subset of the rings counted for it - so no cable is ever
    /// placed on a ring the hit test does not know about. Closing it needs a
    /// pin resolver at every caller, which `resolve_focus_target` does not
    /// have.
    pub(super) fn anchor_rings(&self, pending: Option<PendingRoute>) -> Vec<usize> {
        const MOST: usize = u8::MAX as usize + 1;
        let mut rings = vec![0usize; self.anchors.len()];
        let dropped = |edge: usize, anchor: usize| {
            pending.is_some_and(|p| p.edge == edge && p.detach == Some(anchor))
        };
        let mut count = |anchor: usize| {
            if let Some(rings) = rings.get_mut(anchor) {
                debug_assert!(
                    *rings < MOST,
                    "anchor {anchor} carries more than {MOST} edges; the surplus is not drawn",
                );
                *rings = rings.saturating_add(1).min(MOST);
            }
        };
        for edge in 0..self.edges.len() {
            let route = self.resolved_route(edge);
            for anchor in &route {
                if !dropped(edge, *anchor) {
                    count(*anchor);
                }
            }
            if let Some(pending) = pending
                && pending.edge == edge
                && let Some(anchor) = pending.attach
                && !route.contains(&anchor)
            {
                count(anchor);
            }
        }
        rings
    }

    /// Every edge lowered to the hop chain it draws as, resolved through the
    /// caller's own coordinate space.
    ///
    /// `pin` resolves an endpoint (returning `None` drops the whole edge, since
    /// a cable with one end missing has nowhere to run); `ring` resolves an
    /// orbit's circle, which the draw path takes from the resolved
    /// [`AnchorStyle`](crate::style::AnchorStyle) and the interaction path from
    /// the radii the last frame published; `curve` gives the shape an edge's
    /// legs take, which the orbit search needs because it judges a candidate by
    /// building it.
    ///
    /// Two derivations, in this order:
    ///
    /// The visiting order: each anchor's centre is projected onto the straight
    /// run between the two pins and the wraps are taken in ascending
    /// projection, so a cable passes its anchors in the order it actually
    /// reaches them. A run of zero length leaves nothing to project onto, and
    /// the authored order stands.
    ///
    /// The orbit, in [`orbits::assign`]: which ring each cable takes at each
    /// anchor. Cables sharing an anchor nest by how far each wraps it, shortest
    /// innermost, and where a pair also shares a second anchor - so flies the
    /// corridor between them - candidate orders are built and their crossings
    /// counted, keeping whichever measurably crosses least.
    pub(super) fn edge_hops(
        &self,
        pin: &dyn Fn(&PinRef<I>) -> Option<Station>,
        ring: &dyn Fn(usize, u8) -> Option<edge_path::Orbit>,
        curve: &dyn Fn(usize) -> EdgeCurve,
        phantom: Option<&RoutePhantom>,
    ) -> Vec<CableGeometry<'_, I>> {
        // Resolve every cable's stations and visiting order first. Nothing here
        // needs a radius, which is what lets the orbit be decided afterwards
        // from the whole picture rather than one edge at a time.
        let mut plans: Vec<CablePlan<'_, I>> = Vec::with_capacity(self.edges.len());
        for (index, edge) in self.edges.iter().enumerate() {
            let (Some(a), Some(b)) = (pin(&edge.from), pin(&edge.to)) else {
                continue;
            };
            // Output first, so gradient, arrow and flow follow the data-flow
            // direction however the edge was authored or dragged. Two ends
            // claiming the same direction leave nothing to order by.
            let is_output = |s: &Station| matches!(s.direction, Some(PinDirection::Output));
            let head_is_from = is_output(&a) || !is_output(&b);
            let ((mut head, head_ref), (mut tail, tail_ref)) = if head_is_from {
                ((a, &edge.from), (b, &edge.to))
            } else {
                ((b, &edge.to), (a, &edge.from))
            };

            let phantom = phantom.filter(|p| p.edge == index);
            let route = self.resolved_route(index);
            let mut wraps: Vec<WrapPlan> = Vec::with_capacity(route.len() + 1);
            for &anchor in &route {
                if phantom.and_then(|p| p.exclude) == Some(anchor) {
                    continue;
                }
                // Orbit 0 only to read the centre: an anchor's orbits are
                // concentric, so every one of them has it.
                if let Some(circle) = ring(anchor, 0) {
                    wraps.push(WrapPlan {
                        anchor: Some(anchor),
                        center: circle.center,
                        radius: None,
                        span: 0.0,
                    });
                }
            }
            if let Some(phantom) = phantom {
                match phantom.kind {
                    PhantomKind::At { center, radius } => wraps.push(WrapPlan {
                        anchor: None,
                        center,
                        radius: Some(radius),
                        span: 0.0,
                    }),
                    // Already in the host's route: the attach has round-tripped
                    // and the real wrap above is the one to draw.
                    PhantomKind::Snap { anchor } if !route.contains(&anchor) => {
                        if let Some(circle) = ring(anchor, 0) {
                            wraps.push(WrapPlan {
                                anchor: Some(anchor),
                                center: circle.center,
                                radius: None,
                                span: 0.0,
                            });
                        }
                    }
                    PhantomKind::Snap { .. } => {}
                }
            }

            // A row pin offers two borders and which one is nearer is only
            // decidable with the far end in hand, so the sides are settled
            // here. Both the visiting order and the choice measure against the
            // other end's `aim` rather than its chosen point, so two row pins
            // decide independently and in either order.
            let (head_aim, tail_aim) = (head.aim(), tail.aim());
            let run = [tail_aim[0] - head_aim[0], tail_aim[1] - head_aim[1]];
            let len2 = run[0] * run[0] + run[1] * run[1];
            if len2 >= 1e-6 {
                let projection = |c: &[f32; 2]| {
                    ((c[0] - head_aim[0]) * run[0] + (c[1] - head_aim[1]) * run[1]) / len2
                };
                wraps.sort_by(|a, b| {
                    projection(&a.center)
                        .total_cmp(&projection(&b.center))
                        .then(
                            a.anchor
                                .unwrap_or(usize::MAX)
                                .cmp(&b.anchor.unwrap_or(usize::MAX)),
                        )
                });
            }
            // Each end leaves toward the first station it reaches: the wrap
            // nearest it once the route is ordered, or the other pin when the
            // cable wraps nothing.
            head.settle(wraps.first().map_or(tail_aim, |wrap| wrap.center));
            tail.settle(wraps.last().map_or(head_aim, |wrap| wrap.center));
            // Each wrap's span needs its NEIGHBOURS, so it is read once the
            // visiting order is settled.
            for i in 0..wraps.len() {
                let prev = if i == 0 {
                    head.point
                } else {
                    wraps[i - 1].center
                };
                let next = wraps.get(i + 1).map_or(tail.point, |w| w.center);
                wraps[i].span = wrap_span(wraps[i].center, prev, next);
            }

            plans.push(CablePlan {
                edge: index,
                head,
                tail,
                ends: (head_ref, tail_ref),
                wraps,
            });
        }

        // The orbits come from the whole picture at once: an anchor's order is
        // not separable from its neighbours', because two cables flying the same
        // stretch from one anchor to the next stay apart only while their
        // nesting agrees at both ends.
        let wraps: Vec<Vec<orbits::Wrap>> = plans
            .iter()
            .map(|plan| {
                plan.wraps
                    .iter()
                    .map(|wrap| orbits::Wrap {
                        anchor: wrap.anchor,
                        span: wrap.span,
                    })
                    .collect()
            })
            .collect();
        // How many rings each anchor shows, which is how far out its geometry
        // reaches - the bound a crossing has to clear to count as being in the
        // open space between two anchors rather than at a wrap.
        let mut rings_at = vec![0u8; self.anchors.len()];
        for wraps in &wraps {
            for wrap in wraps {
                if let Some(anchor) = wrap.anchor
                    && let Some(count) = rings_at.get_mut(anchor)
                {
                    *count = count.saturating_add(1);
                }
            }
        }
        let contested = orbits::contested(&wraps, self.anchors.len());
        // Only pairs sharing TWO anchors can cross in a corridor, and a corridor
        // crossing is the only thing measured, so those pairs are the whole
        // search space. A pair meeting at one anchor is settled by containment.
        //
        // The bands a pair's crossings are judged against depend on the anchors
        // it shares and how many rings each of those shows, neither of which a
        // candidate can change, so they are built once here rather than per
        // measurement.
        let mut corridors: Vec<(usize, usize, Vec<edge_path::Corridor>)> = Vec::new();
        // No contested anchor means no pair shares two, so the scan below would
        // find nothing. Skipping it keeps a graph of unrouted cables off an
        // all-pairs walk it can never learn anything from.
        let riding: Vec<usize> = if contested.is_empty() {
            Vec::new()
        } else {
            (0..plans.len())
                .filter(|&slot| {
                    wraps[slot]
                        .iter()
                        .filter(|wrap| wrap.anchor.is_some())
                        .count()
                        > 1
                })
                .collect()
        };
        for (at, &one) in riding.iter().enumerate() {
            for &other in &riding[at + 1..] {
                let (plan, partner) = (&plans[one], &plans[other]);
                let shared = plan.shared_anchors(partner);
                if shared.len() < 2 {
                    continue;
                }
                let reach = |anchor: usize| {
                    ring(
                        anchor,
                        rings_at.get(anchor).copied().unwrap_or(1).saturating_sub(1),
                    )
                };
                let mut bands = Vec::new();
                for leg in shared.windows(2) {
                    if let (Some(from), Some(to)) = (reach(leg[0]), reach(leg[1])) {
                        bands.push(edge_path::Corridor { from, to });
                    }
                }
                if !bands.is_empty() {
                    corridors.push((one, other, bands));
                }
            }
        }
        let mut movable: Vec<usize> = corridors
            .iter()
            .flat_map(|&(one, other, _)| [one, other])
            .collect();
        movable.sort_unstable();
        movable.dedup();
        // A candidate costs one build per movable cable plus one crossing count
        // per band, which is what the search's budget is measured in.
        let per_candidate = movable.len()
            + corridors
                .iter()
                .map(|(_, _, bands)| bands.len())
                .sum::<usize>();
        // What a candidate arrangement actually does, built and counted rather
        // than predicted: whether two cables cross along a corridor also depends
        // on which way each wraps each end, and that is chosen by the geometry
        // from the radii, so it is not knowable before the rings are.
        let mut cost = |arrangement: &[Vec<u8>]| {
            let flattened: Vec<(usize, Vec<[f32; 2]>)> = movable
                .iter()
                .map(|&slot| {
                    let (hops, _) = plans[slot].chain(&arrangement[slot], ring);
                    let path = edge_path::build(&hops, &curve(plans[slot].edge)).path;
                    (slot, edge_path::polyline(&path))
                })
                .collect();
            let chords = |slot: usize| {
                flattened
                    .binary_search_by_key(&slot, |&(at, _)| at)
                    .ok()
                    .map(|at| flattened[at].1.as_slice())
            };
            let mut crossings = 0;
            for (one, other, bands) in &corridors {
                let (Some(first), Some(second)) = (chords(*one), chords(*other)) else {
                    continue;
                };
                crossings += edge_path::crossings_between_flattened(first, second, bands);
            }
            crossings
        };
        let assigned = orbits::assign(
            &wraps,
            self.anchors.len(),
            &contested,
            orbits::budget(movable.len(), per_candidate),
            &mut cost,
        );

        plans
            .iter()
            .enumerate()
            .map(|(slot, plan)| {
                let (hops, rings) = plan.chain(&assigned[slot], ring);
                CableGeometry {
                    edge: plan.edge,
                    hops,
                    rings,
                    ends: plan.ends,
                }
            })
            .collect()
    }

    /// The anchors a route drag on `edge` may snap to.
    ///
    /// Every anchor the edge does not already wrap, plus `detached` - the one a
    /// wrap grab just pulled off. That one stays eligible because the detach may
    /// not have round-tripped yet, and a drag that cannot put an anchor back
    /// where it came from would be a trap.
    pub(super) fn route_snap_eligible(&self, edge: usize, detached: Option<usize>) -> Vec<usize> {
        let route = self.resolved_route(edge);
        (0..self.anchors.len())
            .filter(|a| !route.contains(a) || detached == Some(*a))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_graph::{DEFAULT_ORBIT_OFFSET, DEFAULT_ORBIT_SPACING, anchor, edge};
    use iced_widget::core::Point;

    /// A vocabulary whose ids are all `usize`, so a node, an anchor and an edge
    /// can be told apart by value in a failure message.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    struct AllUsize;

    impl Ids for AllUsize {
        type NodeId = usize;
        type PinId = usize;
        type EdgeId = usize;
        type AnchorId = usize;
        type Payload = ();
    }

    type Graph<'a> =
        NodeGraph<'a, AllUsize, (), iced_widget::core::Theme, iced_widget::renderer::Renderer>;

    /// Node 0 carries the output at the origin, node 1 the input 400 to the
    /// right, so the run is the positive x axis and a projection is just an x
    /// coordinate.
    fn station(pin: &PinRef<AllUsize>) -> Option<Station> {
        match pin.node_id {
            0 => Some(Station::at(
                [0.0, 0.0],
                Border::Right,
                Some(PinDirection::Output),
            )),
            1 => Some(Station::at(
                [400.0, 0.0],
                Border::Bottom,
                Some(PinDirection::Input),
            )),
            _ => None,
        }
    }

    /// Every edge on the default curve, which is what a host that never styles
    /// one gets.
    fn curve(_edge: usize) -> EdgeCurve {
        EdgeCurve::default()
    }

    /// Rings of a fixed radius, so a test reads the ORDER of the wraps rather
    /// than their size.
    fn ring<'g>(graph: &'g Graph<'_>) -> impl Fn(usize, u8) -> Option<edge_path::Orbit> + 'g {
        move |anchor, _orbit| {
            let position = graph.anchors.get(anchor)?.position;
            Some(edge_path::Orbit {
                center: [position.x, position.y],
                radius: 16.0,
            })
        }
    }

    /// Rings whose radius encodes the orbit index, so a test can read WHICH
    /// orbit a wrap was given rather than only where it sits.
    fn indexed_ring<'g>(
        graph: &'g Graph<'_>,
    ) -> impl Fn(usize, u8) -> Option<edge_path::Orbit> + 'g {
        move |anchor, orbit| {
            let position = graph.anchors.get(anchor)?.position;
            Some(edge_path::Orbit {
                center: [position.x, position.y],
                radius: 100.0 + orbit as f32,
            })
        }
    }

    /// The orbit index each wrap was given, read back out of `indexed_ring`.
    fn wrap_orbits(hops: &[edge_path::Hop]) -> Vec<u8> {
        hops.iter()
            .filter_map(|hop| match hop {
                edge_path::Hop::Wrap { orbit } => Some((orbit.radius - 100.0) as u8),
                edge_path::Hop::Pin { .. } => None,
            })
            .collect()
    }

    /// Cables whose geometry gives no reason to prefer one over the other fall
    /// back to edge index, and a snapped drag previews the orbit that tie will
    /// hand it.
    ///
    /// Every cable here runs between the SAME two pins, so every span is equal
    /// and only the index is left to order by. A drag onto an anchor that
    /// already carries a higher-indexed edge is therefore inserted ahead of it
    /// and both cables move. Predicting the free slot past the last instead
    /// would preview the cable on a ring it will not sit on.
    #[test]
    fn a_snap_phantom_takes_the_orbit_the_tie_gives() {
        // The dragged edge is the HIGHEST index: tie order and free slot agree.
        let mut trailing = graph_with_anchors(&[(10, 200.0)]);
        trailing = trailing.push_edge(edge(0, pin_ref(0), pin_ref(1)).route([10]));
        trailing = trailing.push_edge(edge(1, pin_ref(0), pin_ref(1)).route([10]));
        trailing = trailing.push_edge(edge(2, pin_ref(0), pin_ref(1)));
        let phantom = RoutePhantom {
            edge: 2,
            exclude: None,
            kind: PhantomKind::Snap { anchor: 0 },
        };
        assert_eq!(trailing.anchor_rings(Some(phantom.pending())), vec![3]);
        let ring = indexed_ring(&trailing);
        let cables = trailing.edge_hops(&station, &ring, &curve, Some(&phantom));
        assert_eq!(wrap_orbits(&cables[0].hops), vec![0]);
        assert_eq!(wrap_orbits(&cables[1].hops), vec![1]);
        assert_eq!(wrap_orbits(&cables[2].hops), vec![2]);

        // The dragged edge is the LOWEST index: it takes orbit 0 and pushes the
        // resident cable outward. This is the case a free-slot prediction gets
        // wrong, and it is the one the drag then measures its unsnap against.
        let mut leading = graph_with_anchors(&[(10, 200.0)]);
        leading = leading.push_edge(edge(0, pin_ref(0), pin_ref(1)));
        leading = leading.push_edge(edge(1, pin_ref(0), pin_ref(1)).route([10]));
        let phantom = RoutePhantom {
            edge: 0,
            exclude: None,
            kind: PhantomKind::Snap { anchor: 0 },
        };
        assert_eq!(
            leading.anchor_rings(Some(phantom.pending())),
            vec![2],
            "the pending attach is counted before the host applies it",
        );
        let ring = indexed_ring(&leading);
        let cables = leading.edge_hops(&station, &ring, &curve, Some(&phantom));
        assert_eq!(
            wrap_orbits(&cables[0].hops),
            vec![0],
            "the dragged cable previews on the orbit the tie earns it",
        );
        assert_eq!(
            wrap_orbits(&cables[1].hops),
            vec![1],
            "the resident cable previews where the attach will push it",
        );
    }

    /// Two cables through one anchor take the ring the geometry asks for: the
    /// one that wraps LESS sits inside.
    ///
    /// Nested angular intervals are the case that matters, and the common one:
    /// two cables that enter an anchor from the same side and leave to the same
    /// side have one interval containing the other. Put the wider wrap on the
    /// inner ring and its legs have to cut across the narrower cable twice, for
    /// no reason - the same two cables nested the other way round do not cross
    /// at all. Push order knows nothing about that, so here it disagrees.
    #[test]
    fn nested_wraps_put_the_narrower_cable_inside() {
        // Both cables pass above the anchor, the second one closer, so the
        // first's angular interval contains the second's: 157 degrees against
        // 90, around the same centre.
        let stations = |pin: &PinRef<AllUsize>| {
            let (point, side, direction) = match pin.node_id {
                0 => ([-100.0, -20.0], Border::Right, PinDirection::Output),
                1 => ([100.0, -20.0], Border::Bottom, PinDirection::Input),
                2 => ([-100.0, -100.0], Border::Right, PinDirection::Output),
                3 => ([100.0, -100.0], Border::Bottom, PinDirection::Input),
                _ => return None,
            };
            Some(Station::at(point, side, Some(direction)))
        };

        let mut graph = Graph::default();
        graph = graph.push_anchor(anchor(10, Point::new(0.0, 0.0)));
        // The WIDER cable is pushed first, so push order asks for the crossing.
        graph = graph.push_edge(edge(0, pin_ref(0), pin_ref(1)).route([10]));
        graph = graph.push_edge(edge(1, pin_ref(2), pin_ref(3)).route([10]));

        let ring = indexed_ring(&graph);
        let cables = graph.edge_hops(&stations, &ring, &curve, None);
        assert_eq!(
            wrap_orbits(&cables[1].hops),
            vec![0],
            "the narrower wrap belongs on the inner ring",
        );
        assert_eq!(
            wrap_orbits(&cables[0].hops),
            vec![1],
            "the wider wrap goes around the narrower one, not through it",
        );
    }

    /// The centre of every wrap hop, in the order the cable meets them.
    fn wrap_centers(hops: &[edge_path::Hop]) -> Vec<[f32; 2]> {
        hops.iter()
            .filter_map(|hop| match hop {
                edge_path::Hop::Wrap { orbit } => Some(orbit.center),
                edge_path::Hop::Pin { .. } => None,
            })
            .collect()
    }

    fn pin_ref(node: usize) -> PinRef<AllUsize> {
        PinRef::new(node, 0)
    }

    /// Two anchors and an edge routed through both, wired to `station`'s pins.
    fn graph_with_anchors(positions: &[(usize, f32)]) -> Graph<'static> {
        let mut graph = Graph::default();
        for &(id, x) in positions {
            graph = graph.push_anchor(anchor(id, Point::new(x, 0.0)));
        }
        graph
    }

    /// The visiting order is geometry, not authoring order: the host may keep a
    /// route in any order at all, and the cable still passes its anchors in the
    /// order it reaches them.
    #[test]
    fn wraps_are_visited_in_projection_order() {
        let mut graph = graph_with_anchors(&[(10, 300.0), (11, 100.0), (12, 200.0)]);
        graph = graph.push_edge(edge(0, pin_ref(0), pin_ref(1)).route([10, 11, 12]));

        let ring = ring(&graph);
        let cables = graph.edge_hops(&station, &ring, &curve, None);

        assert_eq!(cables.len(), 1);
        assert_eq!(
            wrap_centers(&cables[0].hops),
            vec![[100.0, 0.0], [200.0, 0.0], [300.0, 0.0]],
            "wraps should run along the pin-to-pin run, whatever order they were authored in"
        );
    }

    /// A cable arriving from the input side is turned round before the
    /// projection is taken, so the same scene reads the same either way.
    #[test]
    fn an_input_first_edge_is_oriented_before_ordering() {
        let mut graph = graph_with_anchors(&[(10, 300.0), (11, 100.0)]);
        graph = graph.push_edge(edge(0, pin_ref(1), pin_ref(0)).route([10, 11]));

        let ring = ring(&graph);
        let cables = graph.edge_hops(&station, &ring, &curve, None);

        assert_eq!(
            wrap_centers(&cables[0].hops),
            vec![[100.0, 0.0], [300.0, 0.0]]
        );
        assert_eq!(
            (cables[0].ends.0.node_id, cables[0].ends.1.node_id),
            (0, 1),
            "the output pin heads the cable however the edge was authored"
        );
    }

    /// One orbit takes one edge: the edges through an anchor fill its orbits in
    /// push order, so no two cables share a ring.
    #[test]
    fn edges_through_one_anchor_take_successive_orbits() {
        let mut graph = graph_with_anchors(&[(10, 200.0)]);
        graph = graph.push_edge(edge(0, pin_ref(0), pin_ref(1)).route([10]));
        graph = graph.push_edge(edge(1, pin_ref(0), pin_ref(1)).route([10]));

        assert_eq!(graph.anchor_rings(None), vec![2]);

        let ring = ring(&graph);
        let cables = graph.edge_hops(&station, &ring, &curve, None);

        let orbits: Vec<u8> = cables
            .iter()
            .flat_map(|cable| cable.rings.iter().map(|(_, (_, orbit))| *orbit))
            .collect();
        assert_eq!(orbits, vec![0, 1]);
    }

    /// A route is a set of anchors, so a repeat is one wrap and an id naming
    /// nothing is no wrap at all. A host mid-edit must not be able to make the
    /// widget draw a cable through the same ring twice.
    #[test]
    fn a_route_dedupes_and_drops_unknown_ids() {
        let mut graph = graph_with_anchors(&[(10, 200.0)]);
        graph = graph.push_edge(edge(0, pin_ref(0), pin_ref(1)).route([10, 999, 10]));

        assert_eq!(graph.resolved_route(0), vec![0]);
        assert_eq!(graph.anchor_rings(None), vec![1]);
    }

    /// A node id names a node, not an anchor, even though the two share one id
    /// space - so routing through a node id draws nothing rather than wrapping
    /// the node.
    #[test]
    fn a_node_id_in_a_route_resolves_to_no_wrap() {
        let mut graph = graph_with_anchors(&[(10, 200.0)]);
        graph = graph.push_edge(edge(0, pin_ref(0), pin_ref(1)).route([10]));
        assert_eq!(graph.anchor_index(&10), Some(0));
        assert_eq!(graph.anchor_index(&0), None);
        assert_eq!(graph.resolved_route(0), vec![0]);
    }

    /// The anchor a detach was just published for stays reachable, so a drag can
    /// put it back where it came from before the host has caught up.
    #[test]
    fn a_detached_anchor_stays_snap_eligible() {
        let mut graph = graph_with_anchors(&[(10, 100.0), (11, 200.0)]);
        graph = graph.push_edge(edge(0, pin_ref(0), pin_ref(1)).route([10]));

        assert_eq!(graph.route_snap_eligible(0, None), vec![1]);
        assert_eq!(graph.route_snap_eligible(0, Some(0)), vec![0, 1]);
    }

    /// An unrouted edge is the plain two-station cable, which is what keeps a
    /// graph with no anchors drawing exactly as it did without the feature.
    #[test]
    fn an_unrouted_edge_lowers_to_two_pins() {
        let mut graph = Graph::default();
        graph = graph.push_edge(edge(0, pin_ref(0), pin_ref(1)));

        let ring = ring(&graph);
        let cables = graph.edge_hops(&station, &ring, &curve, None);

        assert_eq!(cables[0].hops.len(), 2);
        assert!(wrap_centers(&cables[0].hops).is_empty());
        assert!(cables[0].rings.is_empty());
    }

    /// A phantom wrap is ordered by the same projection as a real one, so the
    /// preview runs where the committed cable will.
    #[test]
    fn a_phantom_wrap_is_ordered_like_a_real_one() {
        let mut graph = graph_with_anchors(&[(10, 300.0)]);
        graph = graph.push_edge(edge(0, pin_ref(0), pin_ref(1)).route([10]));

        let ring = ring(&graph);
        let phantom = RoutePhantom {
            edge: 0,
            exclude: None,
            kind: PhantomKind::At {
                center: [100.0, 0.0],
                radius: 16.0,
            },
        };
        let cables = graph.edge_hops(&station, &ring, &curve, Some(&phantom));

        assert_eq!(
            wrap_centers(&cables[0].hops),
            vec![[100.0, 0.0], [300.0, 0.0]]
        );
        assert_eq!(
            cables[0].rings,
            vec![(2, (0, 0))],
            "only the host's own wrap names an anchor; the phantom names none"
        );
    }

    /// An anchor a detach was published for leaves the preview at once, so the
    /// cable does not snap back to it for the frame the host takes to apply.
    #[test]
    fn an_excluded_anchor_leaves_the_preview() {
        let mut graph = graph_with_anchors(&[(10, 300.0)]);
        graph = graph.push_edge(edge(0, pin_ref(0), pin_ref(1)).route([10]));

        let ring = ring(&graph);
        let phantom = RoutePhantom {
            edge: 0,
            exclude: Some(0),
            kind: PhantomKind::At {
                center: [100.0, 0.0],
                radius: 16.0,
            },
        };
        let cables = graph.edge_hops(&station, &ring, &curve, Some(&phantom));

        assert_eq!(wrap_centers(&cables[0].hops), vec![[100.0, 0.0]]);
        assert!(cables[0].rings.is_empty());
    }

    /// A snap phantom stands for a real anchor at a real orbit, so it names one
    /// where a ring held at the cursor does not. It stands down once the host's
    /// route carries the anchor itself and the committed wrap takes over.
    #[test]
    fn a_snap_phantom_stands_down_once_the_route_carries_it() {
        let mut graph = graph_with_anchors(&[(10, 200.0)]);
        graph = graph.push_edge(edge(0, pin_ref(0), pin_ref(1)));

        let offered_ring = ring(&graph);
        let phantom = RoutePhantom {
            edge: 0,
            exclude: None,
            kind: PhantomKind::Snap { anchor: 0 },
        };
        let previewed = graph.edge_hops(&station, &offered_ring, &curve, Some(&phantom));
        assert_eq!(wrap_centers(&previewed[0].hops), vec![[200.0, 0.0]]);
        assert_eq!(
            previewed[0].rings,
            vec![(1, (0, 0))],
            "an offered ring still names the anchor it is offered by"
        );

        let mut applied = graph_with_anchors(&[(10, 200.0)]);
        applied = applied.push_edge(edge(0, pin_ref(0), pin_ref(1)).route([10]));
        let applied_ring = ring(&applied);
        let cables = applied.edge_hops(&station, &applied_ring, &curve, Some(&phantom));
        assert_eq!(wrap_centers(&cables[0].hops), vec![[200.0, 0.0]]);
        assert_eq!(cables[0].rings, vec![(1, (0, 0))]);
    }

    /// A run of no length leaves nothing to project onto, so the authored order
    /// is all there is to go on.
    #[test]
    fn a_degenerate_run_keeps_the_authored_order() {
        let mut graph = graph_with_anchors(&[(10, 300.0), (11, 100.0)]);
        graph = graph.push_edge(edge(0, pin_ref(2), pin_ref(2)).route([10, 11]));

        let collapsed = |_: &PinRef<AllUsize>| {
            Some(Station::at(
                [50.0, 50.0],
                Border::Right,
                Some(PinDirection::Output),
            ))
        };
        let ring = ring(&graph);
        let cables = graph.edge_hops(&collapsed, &ring, &curve, None);

        assert_eq!(
            wrap_centers(&cables[0].hops),
            vec![[300.0, 0.0], [100.0, 0.0]]
        );
    }

    /// An endpoint the frame cannot resolve drops the whole cable: half a cable
    /// is not a thing to draw.
    #[test]
    fn an_unresolvable_endpoint_drops_the_edge() {
        let mut graph = Graph::default();
        graph = graph.push_edge(edge(0, pin_ref(0), pin_ref(7)));

        let ring = ring(&graph);
        assert!(graph.edge_hops(&station, &ring, &curve, None).is_empty());
    }

    /// A real corridor is cleared: two cables that fly the stretch between two
    /// anchors come out not crossing along it, where containment alone crosses.
    ///
    /// The scene is the shape the styling demo shows. Both cables wrap both
    /// anchors, and both turn one way at the first and the other at the second -
    /// an S through the corridor, so each rides the CROSSED tangent between the
    /// rings. That is the case where matching ring order is the arrangement that
    /// crosses, and the assignment cannot know it without building: the wrap
    /// direction is chosen by the geometry from the radii. So this asserts on
    /// crossings counted off the built paths, which is also what the search
    /// itself minimises.
    ///
    /// Both halves matter. Clearing the corridor is the feature; containment
    /// crossing is what proves the scene is a genuine conflict rather than one
    /// the seed already happened to solve.
    #[test]
    fn a_crossed_tangent_corridor_comes_out_clear() {
        const A: [f32; 2] = [300.0, 300.0];
        const B: [f32; 2] = [550.0, 300.0];
        // Two sources left of the first anchor, two sinks right of the second,
        // at different heights so the pair has a reason to nest either way.
        let stations = |pin: &PinRef<AllUsize>| {
            let (point, side, direction) = match pin.node_id {
                0 => ([260.0, 193.0], Border::Right, PinDirection::Output),
                1 => ([600.0, 463.0], Border::Bottom, PinDirection::Input),
                2 => ([260.0, 215.0], Border::Right, PinDirection::Output),
                3 => ([600.0, 323.0], Border::Bottom, PinDirection::Input),
                _ => return None,
            };
            Some(Station::at(point, side, Some(direction)))
        };

        let mut graph = Graph::default();
        graph = graph.push_anchor(anchor(10, Point::new(A[0], A[1])));
        graph = graph.push_anchor(anchor(11, Point::new(B[0], B[1])));
        graph = graph.push_edge(edge(0, pin_ref(0), pin_ref(1)).route([10, 11]));
        graph = graph.push_edge(edge(1, pin_ref(2), pin_ref(3)).route([10, 11]));

        // The shipped radii, so the rings sit where a host sees them.
        let rings = |anchor: usize, orbit: u8| {
            let position = graph.anchors.get(anchor)?.position;
            Some(edge_path::Orbit {
                center: [position.x, position.y],
                radius: DEFAULT_ORBIT_OFFSET + orbit as f32 * DEFAULT_ORBIT_SPACING,
            })
        };

        // Crossings along the corridor for one arrangement, measured the way the
        // search measures them.
        let corridor_crossings = |arrangement: &[Vec<u8>]| {
            let cables = graph.edge_hops(&stations, &rings, &curve, None);
            let paths: Vec<edge_path::EdgePath> = cables
                .iter()
                .enumerate()
                .map(|(slot, cable)| {
                    let mut hops = cable.hops.clone();
                    for (&(hop, (_, _)), &orbit) in cable.rings.iter().zip(&arrangement[slot]) {
                        if let edge_path::Hop::Wrap { orbit: circle } = &mut hops[hop] {
                            circle.radius =
                                DEFAULT_ORBIT_OFFSET + orbit as f32 * DEFAULT_ORBIT_SPACING;
                        }
                    }
                    edge_path::build(&hops, &EdgeCurve::default()).path
                })
                .collect();
            let band = |anchor: usize| rings(anchor, 1).expect("two cables per anchor");
            let bands = [edge_path::Corridor {
                from: band(0),
                to: band(1),
            }];
            edge_path::crossings_between(&paths[0], &paths[1], &bands)
        };

        let chosen: Vec<Vec<u8>> = graph
            .edge_hops(&stations, &rings, &curve, None)
            .iter()
            .map(|cable| cable.rings.iter().map(|&(_, (_, orbit))| orbit).collect())
            .collect();
        assert_eq!(
            corridor_crossings(&chosen),
            0,
            "the corridor still crosses at {chosen:?}",
        );
        assert!(
            corridor_crossings(&[vec![0, 0], vec![1, 1]]) > 0,
            "matching ring order does not cross here, so the scene proves nothing",
        );
    }

    /// The styling demo's own scene comes out clear, and needs more than the
    /// obvious exchange to get there.
    ///
    /// Three cables on each anchor: one pair flies the corridor between them and
    /// a third cable wraps each end on its own way elsewhere. Containment seats
    /// the pair to disagree across the corridor, so it crosses. What makes this
    /// worth a test of its own is that the seed is a PLATEAU: exchanging the two
    /// adjacent rings the pair sits on leaves the count at one, and only
    /// exchanging a non-adjacent pair - moving the uninvolved third cable out of
    /// the way - reaches a clear corridor. A search confined to adjacent rings
    /// stalls here with the crossing still on screen.
    #[test]
    fn a_three_cable_corridor_comes_out_clear() {
        let stations = |pin: &PinRef<AllUsize>| {
            let (point, side, direction) = match pin.node_id {
                0 => ([260.0, 193.3], Border::Right, PinDirection::Output),
                1 => ([350.0, 243.3], Border::Bottom, PinDirection::Input),
                2 => ([510.0, 243.3], Border::Right, PinDirection::Output),
                3 => ([600.0, 193.3], Border::Bottom, PinDirection::Input),
                4 => ([600.0, 463.3], Border::Bottom, PinDirection::Input),
                5 => ([280.0, 103.3], Border::Right, PinDirection::Output),
                6 => ([600.0, 323.3], Border::Bottom, PinDirection::Input),
                _ => return None,
            };
            Some(Station::at(point, side, Some(direction)))
        };
        let mut graph = Graph::default();
        graph = graph.push_anchor(anchor(10, Point::new(300.0, 300.0)));
        graph = graph.push_anchor(anchor(11, Point::new(550.0, 300.0)));
        graph = graph.push_edge(edge(0, pin_ref(0), pin_ref(1)).route([10]));
        graph = graph.push_edge(edge(1, pin_ref(2), pin_ref(3)).route([11]));
        graph = graph.push_edge(edge(2, pin_ref(0), pin_ref(4)).route([10, 11]));
        graph = graph.push_edge(edge(3, pin_ref(5), pin_ref(6)).route([10, 11]));

        let rings = |anchor: usize, orbit: u8| {
            let position = graph.anchors.get(anchor)?.position;
            Some(edge_path::Orbit {
                center: [position.x, position.y],
                radius: DEFAULT_ORBIT_OFFSET + orbit as f32 * DEFAULT_ORBIT_SPACING,
            })
        };
        let corridor_crossings =
            |arrangement: &[Vec<u8>]| corridor_count(&graph, &stations, &rings, arrangement);

        let chosen: Vec<Vec<u8>> = graph
            .edge_hops(&stations, &rings, &curve, None)
            .iter()
            .map(|cable| cable.rings.iter().map(|&(_, (_, orbit))| orbit).collect())
            .collect();
        assert_eq!(
            corridor_crossings(&chosen),
            0,
            "the corridor still crosses at {chosen:?}",
        );
        // Containment: at the first anchor the pair sits 2 and 1, at the second 1
        // and 2, so it disagrees across the corridor.
        let containment = vec![vec![0], vec![0], vec![2, 1], vec![1, 2]];
        assert!(
            corridor_crossings(&containment) > 0,
            "containment does not cross here, so the scene proves nothing",
        );
        // Exchanging only the pair's own two rings stays on the plateau.
        assert!(
            corridor_crossings(&[vec![0], vec![0], vec![1, 1], vec![2, 2]]) > 0,
            "the adjacent exchange already clears this, so the scene does not \
             need the wider neighbourhood it exists to justify",
        );
    }

    /// The pin hops of the one cable between two row pins, as
    /// `(anchor, side)` in the order the cable runs them.
    ///
    /// Node 0 carries the output and spans `first` horizontally, node 1 the
    /// input over `second`; both pins sit at y = 0, so only the horizontal
    /// arrangement is in play.
    fn row_pin_hops(first: (f32, f32), second: (f32, f32)) -> Vec<([f32; 2], Border)> {
        let stations = move |pin: &PinRef<AllUsize>| {
            let (borders, direction) = match pin.node_id {
                0 => (first, PinDirection::Output),
                1 => (second, PinDirection::Input),
                _ => return None,
            };
            Some(Station::row(
                [borders.0, 0.0],
                [borders.1, 0.0],
                Some(direction),
            ))
        };
        let graph = Graph::default().push_edge(edge(0, pin_ref(0), pin_ref(1)));
        let ring = ring(&graph);
        let cables = graph.edge_hops(&stations, &ring, &curve, None);
        cables[0]
            .hops
            .iter()
            .filter_map(|hop| match hop {
                edge_path::Hop::Pin { point, side } => Some((*point, *side)),
                edge_path::Hop::Wrap { .. } => None,
            })
            .collect()
    }

    /// A cable between two row pins leaves each node by the border facing the
    /// other one, tangent outward on that side: out of the left node's right
    /// border, into the right node's left border, and the other way round once
    /// the nodes trade places.
    ///
    /// Both ends measure against the OTHER pin's centre rather than its choice,
    /// so neither waits on the other and the pair cannot disagree.
    #[test]
    fn a_cable_between_row_pins_takes_the_facing_borders() {
        let right = Border::Right;
        let left = Border::Left;

        assert_eq!(
            row_pin_hops((0.0, 100.0), (300.0, 400.0)),
            vec![([100.0, 0.0], right), ([300.0, 0.0], left)],
        );
        assert_eq!(
            row_pin_hops((300.0, 400.0), (0.0, 100.0)),
            vec![([300.0, 0.0], left), ([100.0, 0.0], right)],
        );
    }

    /// Two row pins sharing a centre line leave nothing to prefer, and the tie
    /// keeps the left border at both ends.
    ///
    /// The choice turns only on which side of a node's centre the far end lies,
    /// so overlapping nodes settle on one answer and hold it until that centre
    /// is actually crossed - no hysteresis, no flicker.
    #[test]
    fn row_pins_over_the_same_centre_line_do_not_waver() {
        let left = Border::Left;
        assert_eq!(
            row_pin_hops((0.0, 100.0), (0.0, 100.0)),
            vec![([0.0, 0.0], left), ([0.0, 0.0], left)],
        );
        // Nodes overlapping by all but a hair still agree with the geometry:
        // the second node's centre is to the right, the first's to the left.
        assert_eq!(
            row_pin_hops((0.0, 100.0), (2.0, 102.0)),
            vec![([100.0, 0.0], Border::Right), ([2.0, 0.0], left)],
        );
    }

    /// Crossings along a corridor for one arrangement, counted the way the
    /// assignment counts them.
    fn corridor_count(
        graph: &Graph<'_>,
        stations: &dyn Fn(&PinRef<AllUsize>) -> Option<Station>,
        rings: &dyn Fn(usize, u8) -> Option<edge_path::Orbit>,
        arrangement: &[Vec<u8>],
    ) -> usize {
        let cables = graph.edge_hops(stations, rings, &curve, None);
        let paths: Vec<edge_path::EdgePath> = cables
            .iter()
            .enumerate()
            .map(|(slot, cable)| {
                let mut hops = cable.hops.clone();
                for (&(hop, _), &orbit) in cable.rings.iter().zip(&arrangement[slot]) {
                    if let edge_path::Hop::Wrap { orbit: circle } = &mut hops[hop] {
                        circle.radius = DEFAULT_ORBIT_OFFSET + orbit as f32 * DEFAULT_ORBIT_SPACING;
                    }
                }
                edge_path::build(&hops, &EdgeCurve::default()).path
            })
            .collect();
        let rings_at: Vec<u8> = (0..graph.anchors.len())
            .map(|anchor| {
                u8::try_from(
                    (0..graph.edges.len())
                        .filter(|&edge| graph.resolved_route(edge).contains(&anchor))
                        .count(),
                )
                .unwrap_or(u8::MAX)
            })
            .collect();
        let mut count = 0;
        for (i, first) in paths.iter().enumerate() {
            for (j, second) in paths.iter().enumerate().skip(i + 1) {
                let mut shared: Vec<usize> = graph
                    .resolved_route(i)
                    .into_iter()
                    .filter(|a| graph.resolved_route(j).contains(a))
                    .collect();
                shared.sort_unstable();
                if shared.len() < 2 {
                    continue;
                }
                let reach = |anchor: usize| {
                    rings(
                        anchor,
                        rings_at.get(anchor).copied().unwrap_or(1).saturating_sub(1),
                    )
                };
                let mut bands = Vec::new();
                for (at, &from) in shared.iter().enumerate() {
                    for &to in &shared[at + 1..] {
                        if let (Some(from), Some(to)) = (reach(from), reach(to)) {
                            bands.push(edge_path::Corridor { from, to });
                        }
                    }
                }
                count += edge_path::crossings_between(first, second, &bands);
            }
        }
        count
    }
}

#[cfg(test)]
mod station_tests {
    use super::{Border, Station};
    use crate::node_pin::{PinDirection, PinSide};

    /// A row pin spanning x in [0, 100] at y = 10, as `pin_positions` hands it
    /// over: left border first, right border second.
    fn row() -> Station {
        Station::for_pin(
            PinSide::Row,
            ([0.0, 10.0], [100.0, 10.0]),
            PinDirection::Both,
        )
    }

    // The border a row pin's cable takes is the one nearer the far end, and the
    // tangent side follows it, so the cable leaves the node outward on the side
    // it attached to instead of running back through the body.
    #[test]
    fn a_row_pin_takes_the_border_nearer_the_far_end() {
        let mut rightward = row();
        rightward.settle([400.0, -900.0]);
        assert_eq!(
            (rightward.point, rightward.side),
            ([100.0, 10.0], Border::Right),
        );

        let mut leftward = row();
        leftward.settle([-400.0, 900.0]);
        assert_eq!((leftward.point, leftward.side), ([0.0, 10.0], Border::Left),);
    }

    // Both borders sit at the same height, so the vertical distance cancels and
    // only the half of the node the far end lies in decides. The flip point is
    // the node's centre line, crossed once, which is why the choice needs no
    // hysteresis; a far end exactly on that line keeps the left border.
    #[test]
    fn the_choice_turns_on_the_node_centre_line_alone() {
        for height in [-1000.0, 0.0, 1000.0] {
            let mut just_right = row();
            just_right.settle([50.1, height]);
            assert_eq!(just_right.side, Border::Right);

            let mut on_the_line = row();
            on_the_line.settle([50.0, height]);
            assert_eq!(on_the_line.side, Border::Left);
        }
    }

    // A pin that declares one side has nothing to choose: the side it named
    // stands however the cable runs from it.
    #[test]
    fn a_one_sided_pin_keeps_the_side_it_declared() {
        let mut left = Station::for_pin(
            PinSide::Left,
            ([0.0, 10.0], [0.0, 10.0]),
            PinDirection::Input,
        );
        left.settle([400.0, 10.0]);
        assert_eq!((left.point, left.side), ([0.0, 10.0], Border::Left));
    }
}

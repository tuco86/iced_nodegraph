//! Programmatic camera framing: [`focus`] builds a [`Task`] that frames a
//! [`FocusTarget`] in the graph carrying a given [`widget::Id`].
//!
//! The same shape as `text_input::focus` and `scrollable::scroll_to`: the
//! host runs the task from `update`, the runtime walks the widget tree with a
//! [`widget::Operation`], and the graph whose id matches picks the request up.
//! The graph resolves the target to a world rectangle from its live layout
//! and starts the fit on its next update, committing through `on_camera`
//! like any other camera change.

use std::any::Any;
use std::time::Duration;

use iced_runtime::Task;
use iced_widget::core::widget::{self, Operation};
use iced_widget::core::{Padding, Rectangle};

use crate::ids::Ids;

/// What the camera should frame. The host only names ids (or `All`,
/// `Selection`, an explicit world rect); the graph resolves them to a world
/// AABB from its live layout, so the host never computes node bounds itself.
#[derive(Clone, Debug)]
pub enum FocusTarget<I: Ids> {
    /// Every node and anchor in the graph.
    All,
    /// The current selection ([`Node::selected`](crate::Node::selected) or the
    /// widget's working selection). An empty selection is a no-op. Anchors are
    /// never selected, so they never appear here.
    Selection,
    /// A single node by id.
    Node(I::NodeId),
    /// The union of several nodes' bounds.
    Nodes(Vec<I::NodeId>),
    /// A single anchor by id, framed to its outermost ring.
    Anchor(I::AnchorId),
    /// The union of several anchors' bounds.
    Anchors(Vec<I::AnchorId>),
    /// The union of an edge's two endpoint nodes' bounds and every anchor it
    /// routes through (seeing a connection means seeing what it connects and
    /// where it goes).
    Edge(I::EdgeId),
    /// The union of several edges' endpoints and anchors.
    Edges(Vec<I::EdgeId>),
    /// An explicit world-space rectangle, for targets the widget cannot
    /// resolve on its own.
    Rect(Rectangle),
}

/// Options for [`focus`] and the keymap frame actions: padding, zoom bounds,
/// and the optional tween.
#[derive(Clone, Debug)]
pub struct FocusOptions {
    /// Per-side screen-px padding around the fitted bounds.
    pub padding: Padding,
    /// Extra lower zoom bound, intersected with the camera's own zoom floor.
    /// `None` leaves only that floor in effect.
    pub min_zoom: Option<f32>,
    /// Extra upper zoom bound, intersected with the camera's own zoom
    /// ceiling. Defaults to `Some(1.0)` so focusing a single small node fits
    /// it at native size instead of zooming in arbitrarily far.
    pub max_zoom: Option<f32>,
    /// Tween toward the target instead of jumping. `None` jumps immediately.
    pub animation: Option<FocusAnimation>,
}

impl Default for FocusOptions {
    fn default() -> Self {
        Self {
            padding: Padding::new(40.0),
            min_zoom: None,
            max_zoom: Some(1.0),
            animation: Some(FocusAnimation::default()),
        }
    }
}

/// The tween a [`FocusOptions::animation`] runs: duration plus easing curve.
#[derive(Clone, Copy, Debug)]
pub struct FocusAnimation {
    /// How long the camera takes to reach the target. A zero duration
    /// behaves like `animation: None` (an immediate jump).
    pub duration: Duration,
    /// The easing curve driving the interpolation.
    pub easing: Easing,
}

impl Default for FocusAnimation {
    fn default() -> Self {
        Self {
            duration: Duration::from_millis(300),
            easing: Easing::EaseInOutCubic,
        }
    }
}

/// Easing curve for a [`FocusAnimation`], applied to the tween's normalized
/// progress `t` in `[0, 1]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Easing {
    /// No easing: constant velocity.
    Linear,
    /// Slow start and end, fast middle. The default.
    EaseInOutCubic,
    /// Fast start, slow end.
    EaseOutCubic,
}

impl Easing {
    /// Applies the curve to `t`, clamped to `[0, 1]`.
    pub fn apply(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Easing::Linear => t,
            Easing::EaseInOutCubic => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
                }
            }
            Easing::EaseOutCubic => 1.0 - (1.0 - t).powi(3),
        }
    }
}

/// A framing request in flight between the operation and the graph that
/// accepts it.
pub(super) struct FocusRequest<I: Ids> {
    pub target: FocusTarget<I>,
    pub options: FocusOptions,
}

/// Frames `target` in the [`NodeGraph`](crate::NodeGraph) carrying `id`.
///
/// The graph resolves the target against its live layout and fits the camera
/// to it, tweening when `options.animation` is set, and commits the result
/// through [`on_camera`](crate::NodeGraph::on_camera) like any other camera
/// change. An unknown id, an unknown target id or an empty selection is a
/// no-op. A graph without an [`id`](crate::NodeGraph::id) never matches.
///
/// # Examples
///
/// ```rust,no_run
/// use iced::Task;
/// use iced::widget::Id;
/// use iced_nodegraph::{FocusOptions, FocusTarget, Indexed};
///
/// # enum Message {}
/// fn frame_node(id: usize) -> Task<Message> {
///     iced_nodegraph::focus(Id::new("graph"), FocusTarget::<Indexed>::Node(id), FocusOptions::default())
/// }
/// ```
pub fn focus<I: Ids, T>(
    id: impl Into<widget::Id>,
    target: FocusTarget<I>,
    options: FocusOptions,
) -> Task<T> {
    iced_runtime::task::effect(iced_runtime::Action::widget(focus_operation(
        id, target, options,
    )))
}

/// The [`widget::Operation`] behind [`focus`], for hosts and harnesses that
/// run operations themselves (`UserInterface::operate`, or chaining with
/// another operation) rather than through a `Task`.
pub fn focus_operation<I: Ids>(
    id: impl Into<widget::Id>,
    target: FocusTarget<I>,
    options: FocusOptions,
) -> impl Operation + 'static {
    Focus {
        id: id.into(),
        request: Some(FocusRequest { target, options }),
    }
}

/// The operation behind [`focus`]: hands its request to the graph whose id
/// matches, through the `Option<FocusRequest<I>>` slot the graph offers in
/// `Widget::operate`.
struct Focus<I: Ids> {
    id: widget::Id,
    request: Option<FocusRequest<I>>,
}

impl<I: Ids> Operation for Focus<I> {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }

    fn custom(&mut self, id: Option<&widget::Id>, _bounds: Rectangle, state: &mut dyn Any) {
        if id == Some(&self.id)
            && let Some(slot) = state.downcast_mut::<Option<FocusRequest<I>>>()
        {
            *slot = self.request.take();
        }
    }
}

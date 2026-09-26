//! The in-flight Director session.
//!
//! `model` is the HTTP Completer and `harness` is the ACP adapter. Both
//! implement `Completer`. This module is the choice between them and the one
//! cancellable slot per Character Instance. The abandon flag lives with the
//! slot because the worker `Slots` starts is the thread the HTTP reader checks
//! between frames.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;

use ai_buddy_core::director::{self, Completer, Context, ModelDirector, Reply, Wake, WakeRequest};
use ai_buddy_core::roster::InstanceId;

use crate::model::{blank, endpoint_from, tracing, DirectorSettings, Endpoint};

/// Whichever Completer this process has: the attached Harness for every
/// Instance (ADR-0008), else an HTTP `Endpoint` per Instance.
/// An enum so `Endpoint`'s inherent methods keep their type.
pub enum AnyCompleter {
    // Boxed: an `Endpoint` carries the whole session and the agent, and the
    // Harness arm is one `Arc`, so the enum would otherwise be moved around at
    // the size of the larger arm.
    Http(Box<Endpoint>),
    Harness(Arc<crate::harness::Session>),
}

impl Completer for AnyCompleter {
    fn complete(&self, request: &WakeRequest) -> Result<Reply, String> {
        match self {
            AnyCompleter::Http(endpoint) => endpoint.complete(request),
            AnyCompleter::Harness(session) => session.complete(request),
        }
    }
}

/// The Completer `configured` promises. Harness first: with one attached the
/// HTTP settings are not consulted at all.
pub fn completer_from(settings: &DirectorSettings) -> Option<AnyCompleter> {
    match crate::harness::attached() {
        Some(session) => Some(AnyCompleter::Harness(session)),
        None => endpoint_from(settings)
            .map(Box::new)
            .map(AnyCompleter::Http),
    }
}

thread_local! {
    /// The abandon flag for the model call on this thread.
    /// Thread-local, not an `Endpoint` field, because the socket lives in
    /// the worker stack. Cancellation is a Shell resource, not `Completer`.
    static ABANDONED: std::cell::RefCell<Option<Arc<AtomicBool>>> =
        const { std::cell::RefCell::new(None) };
}

/// Whether the call on this thread has been dropped by the frame loop.
/// False when unset, so the probe and the tests need no second path.
pub(crate) fn abandoned() -> bool {
    ABANDONED.with_borrow(|flag| {
        flag.as_ref()
            .is_some_and(|flag| flag.load(Ordering::SeqCst))
    })
}

/// Every session call the app has on the wire. One slot per Character Instance.
/// One registry, not one per Instance. Sessions stay per-Instance inside each
/// `Endpoint` (ADR-0008). There is no cap. N Instances make N calls.
#[derive(Default)]
pub struct Slots {
    slots: HashMap<InstanceId, Slot>,
}

struct Slot {
    /// Which call is this Instance's current one. A reply stamped with any
    /// other number was computed for a moment the Instance has left.
    epoch: u64,
    tx: Sender<Delivered>,
    rx: Receiver<Delivered>,
    /// Raised when the call is superseded. The worker reads it between SSE
    /// frames so an abandoned call closes its connection.
    abandoned: Arc<AtomicBool>,
    waiting: bool,
    /// Whether the call answers something the user did, which is the whole of
    /// what the Thinking ellipsis asks.
    reactive: bool,
}

/// One worker's answer, stamped with the call it belongs to.
struct Delivered {
    epoch: u64,
    answered: Answered,
}

/// What one wake came back with.
/// The proposal is meaningless without the moment that asked for it. The near
/// miss is what tells an undeclared Behavior name from a model that talked.
pub struct Answered {
    pub wake: Wake,
    pub context: Context,
    /// The Behavior name the reply proposed that this Character declares none
    /// of. `None` on every other reply.
    pub near_miss: Option<String>,
    /// The cap ended this turn, so what was said is as far as the model got.
    /// The line the Chat surface remembers is marked with it.
    pub truncated: bool,
}

impl Default for Slot {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            epoch: 0,
            tx,
            rx,
            abandoned: Arc::new(AtomicBool::new(false)),
            waiting: false,
            reactive: false,
        }
    }
}

impl Slot {
    /// Stop treating this slot's in-flight call as this Instance's answer.
    /// The next call gets a fresh abandon flag. The old flag stays with the
    /// worker so it can still close the connection.
    fn supersede(&mut self) {
        self.abandoned.store(true, Ordering::SeqCst);
        self.abandoned = Arc::new(AtomicBool::new(false));
        self.epoch += 1;
        self.waiting = false;
        self.reactive = false;
    }
}

/// The trace line for a proposed Behavior name nobody declared.
/// Carries the declared set so `prowll` beside `prowl` reads as a typo, beside
/// `wave` as a model ignoring the contract. Workers interleave, so the Instance id leads.
pub(crate) fn near_miss_line(id: &str, name: &str, behaviors: &[String]) -> String {
    format!(
        "director: {id} {name} is no declared Behavior; declared: {}",
        behaviors.join(", ")
    )
}

impl Slots {
    pub fn new() -> Self {
        Self::default()
    }

    /// Send this Character Prompt for `id`, abandoning whatever `id` had out.
    /// Infallible. Starting a call is the cancellation of the previous one,
    /// so there is no busy to report. Per-Instance newest-wins.
    pub fn wake<C: Completer + Send + Sync + 'static>(
        &mut self,
        id: &InstanceId,
        director: Arc<ModelDirector<C>>,
        context: Context,
    ) {
        let slot = self.slots.entry(id.clone()).or_default();
        slot.supersede();
        slot.waiting = true;
        slot.reactive = director::reactive(&context.happened);
        let epoch = slot.epoch;
        let tx = slot.tx.clone();
        let abandoned = Arc::clone(&slot.abandoned);
        let traced = id.clone();
        thread::spawn(move || {
            ABANDONED.with_borrow_mut(|flag| *flag = Some(abandoned));
            // Always send. A panic here would leave the slot waiting forever
            // and skip StaticDirector on every later tick.
            let woken = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let woken = director.wake_and_near_miss(&context);
                // Traced here, beside the reply it came from. The Action Log
                // takes it from `take` instead, where a superseded reply has
                // already been dropped.
                if tracing() {
                    if let Some(name) = &woken.near_miss {
                        eprintln!("{}", near_miss_line(&traced, name, director.behaviors()));
                    }
                }
                woken
            }))
            .unwrap_or(director::Woken {
                wake: Wake::Failed,
                near_miss: None,
                truncated: false,
            });
            let _ = tx.send(Delivered {
                epoch,
                answered: Answered {
                    wake: woken.wake,
                    context,
                    near_miss: woken.near_miss,
                    truncated: woken.truncated,
                },
            });
        });
    }

    /// The reply for `id`, with the moment it was computed for.
    /// A superseded moment is dropped here rather than handed out for a
    /// caller to compare.
    pub fn take(&mut self, id: &InstanceId) -> Option<Answered> {
        let slot = self.slots.get_mut(id)?;
        while let Ok(delivered) = slot.rx.try_recv() {
            if delivered.epoch != slot.epoch {
                continue;
            }
            slot.waiting = false;
            slot.reactive = false;
            return Some(delivered.answered);
        }
        None
    }

    /// Drop whatever `id` has on the wire, and forget the Instance.
    /// Character switch, Completer retarget, and dismissal would apply the
    /// wrong buddy's answer. Remove the slot so the registry cannot accumulate.
    pub fn abandon(&mut self, id: &InstanceId) {
        if let Some(slot) = self.slots.remove(id) {
            slot.abandoned.store(true, Ordering::SeqCst);
        }
    }

    /// Whether `id` is waiting on the Director. Not a gate on `wake`. An
    /// observation, for the Static Director standing down while a session
    /// proposal is about to land.
    pub fn waiting(&self, id: &InstanceId) -> bool {
        self.slots.get(id).is_some_and(|slot| slot.waiting)
    }

    /// Whether what `id` is waiting on answers something the user did, which is
    /// the Thinking ellipsis's whole question: a proactive wake stays invisible.
    pub fn thinking(&self, id: &InstanceId) -> bool {
        self.slots
            .get(id)
            .is_some_and(|slot| slot.waiting && slot.reactive)
    }
}

/// Drop an in-flight wake and install a Completer for the new settings.
/// A wake still on the wire would propose against the old host and session.
/// `Slots::abandon` closes the connection so the old host stops generating.
pub fn retarget_model(
    slots: &mut Slots,
    id: &InstanceId,
    model: &mut Option<Arc<ModelDirector<AnyCompleter>>>,
    behaviors: impl IntoIterator<Item = impl Into<String>>,
    character: impl Into<String>,
    settings: &DirectorSettings,
    configured: bool,
) {
    slots.abandon(id);
    *model = configured.then(|| {
        Arc::new(ModelDirector::new(
            completer_from(settings).expect("configured means a Completer exists"),
            behaviors,
            id.clone(),
            character,
            blank(),
        ))
    });
}

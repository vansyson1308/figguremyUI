//! Running steps against the simulator and reporting each one: what it set off, what was
//! expected and didn't hold, and where the prototype stood after it.

use figcraft_doc::design::{Expect, Step, StepAction};
use figcraft_doc::{Document, NodeId};
use serde_json::{Value, json};

use crate::proto_sim::{OverlayInfo, Sim, State, StepOutcome};

/// What shows: the screen and the open overlays, bottom first.
pub(super) struct Showing {
    pub screen: NodeId,
    pub screen_name: String,
    pub overlays: Vec<OverlayInfo>,
}

impl Showing {
    fn of(sim: &Sim) -> Self {
        let name = |id: NodeId| sim.document().node(id).map(|n| n.name.clone()).unwrap_or_default();
        Showing {
            screen: sim.screen(),
            screen_name: name(sim.screen()),
            overlays: sim.overlays().iter().map(|o| OverlayInfo { id: o.id, name: name(o.id) }).collect(),
        }
    }
}

/// One step as it went.
pub(super) struct StepReport {
    pub outcome: StepOutcome,
    /// What the step expected that didn't hold.
    pub failures: Vec<String>,
    /// What showed after the step.
    pub state: Showing,
}

impl StepReport {
    /// Everything wrong with the step: why it couldn't run, then what it expected.
    pub fn problems(&self) -> Vec<String> {
        self.outcome.problem.iter().cloned().chain(self.failures.iter().cloned()).collect()
    }
}

/// A run of steps from a start frame.
pub(super) struct Run {
    /// Every step ran without a problem, every gesture and key set something off, and every
    /// expectation held.
    pub passed: bool,
    /// The steps that ran, in order (a run that stops on a failure leaves the rest out).
    pub steps: Vec<StepReport>,
    /// The first step that failed.
    pub failed_step: Option<usize>,
    /// What the final expectation asked for that didn't hold; when the run couldn't start, why.
    pub failures: Vec<String>,
    /// The state at the end (`None` when the run couldn't start).
    pub state: Option<State>,
}

impl Run {
    /// The failures that decided the run: the first failing step's, else the final expectation's.
    pub fn first_failures(&self) -> Vec<String> {
        match self.failed_step.and_then(|i| self.steps.get(i)) {
            Some(step) => step.problems(),
            None => self.failures.clone(),
        }
    }

    /// Every screen and overlay the run showed: the start, and what showed after each step.
    pub fn shown(&self, start: NodeId) -> Vec<NodeId> {
        let after = self.steps.iter().flat_map(|s| std::iter::once(s.state.screen).chain(s.state.overlays.iter().map(|o| o.id)));
        std::iter::once(start).chain(after).collect()
    }
}

/// A step fails when it couldn't run or an action in it couldn't, when a gesture or key set
/// nothing off, and when what it expects doesn't hold.
fn failed(action: &StepAction, report: &StepReport) -> bool {
    let inert = matches!(action, StepAction::Gesture { .. } | StepAction::Key { .. }) && !report.outcome.fired;
    report.outcome.problem.is_some() || inert || !report.failures.is_empty()
}

/// Play `steps` from `start` on a copy of `doc`, then check `expect`. With `stop_on_failure` the
/// run ends at the first failing step and the final expectation isn't checked (the prototype
/// isn't where the steps meant it to be).
pub(super) fn run(doc: &Document, start: NodeId, steps: &[Step], expect: &Expect, stop_on_failure: bool) -> Run {
    match Sim::new(doc, start) {
        Ok(sim) => play(sim, steps, expect, stop_on_failure),
        Err(why) => Run { passed: false, steps: Vec::new(), failed_step: None, failures: vec![format!("can't start: {why}")], state: None },
    }
}

/// [`run`] on a simulator that has started.
pub(super) fn play(mut sim: Sim, steps: &[Step], expect: &Expect, stop_on_failure: bool) -> Run {
    let mut reports = Vec::new();
    let mut failed_step = None;
    for (i, step) in steps.iter().enumerate() {
        let outcome = sim.step(&step.action);
        let report = StepReport { outcome, failures: sim.check(&step.expect), state: Showing::of(&sim) };
        let stop = failed(&step.action, &report);
        reports.push(report);
        if stop {
            failed_step = failed_step.or(Some(i));
            if stop_on_failure {
                break;
            }
        }
    }
    let stopped = stop_on_failure && failed_step.is_some();
    let failures = if stopped { Vec::new() } else { sim.check(expect) };
    Run { passed: failed_step.is_none() && failures.is_empty(), steps: reports, failed_step, failures, state: Some(sim.state()) }
}

/// What a simulated run doesn't model: said in every result, so that a pass isn't read as "the
/// player was watched doing it".
pub(super) const NOT_MODELLED: [&str; 5] = [
    "animation and timing (transitions, easing, how long a delay is)",
    "scroll position (scroll-to changes nothing; a layer scrolled or clipped out of view still takes a gesture)",
    "drag direction",
    "the device frame and responsive resizing (frames keep the size they have in the file)",
    "pointer hit-testing (a gesture names its layer; what overlaps it on screen isn't considered)",
];

/// The steps of a run as `proto.simulate` reports them.
pub(super) fn steps_json(steps: &[Step], run: &Run) -> Vec<Value> {
    run.steps
        .iter()
        .zip(steps)
        .enumerate()
        .map(|(index, (r, step))| {
            let mut v = json!({
                "index": index,
                "action": step.action,
                "fired": r.outcome.fired,
                "effects": r.outcome.effects,
                "failures": r.failures,
                "state": {"screen": r.state.screen, "screenName": r.state.screen_name, "overlays": r.state.overlays},
            });
            if let (Some(o), Some(problem)) = (v.as_object_mut(), &r.outcome.problem) {
                o.insert("problem".into(), json!(problem));
            }
            if let (Some(o), false) = (v.as_object_mut(), step.note.is_empty()) {
                o.insert("note".into(), json!(step.note));
            }
            v
        })
        .collect()
}

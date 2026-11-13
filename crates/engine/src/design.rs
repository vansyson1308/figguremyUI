//! The design context commands (`design.*`): the brief, assumptions, decisions, findings from
//! checks and reviews, and alternatives. The data is [`figcraft_doc::design::DesignContext`].
//!
//! The commands keep apart what was measured, what somebody thinks and what a person said, and
//! who may settle each: a check is settled by running its rule again, an opinion or a person's
//! feedback by a person. An AI agent (a command run through [`crate::Session::execute_as`]) can
//! propose, record and claim a fix; it can't accept its own decision or end state, choose among
//! alternatives, confirm that feedback is met, record that an assumption about people was borne
//! out, or change or remove what a person wrote or settled. Each record says who made it, and
//! who passed it on or edited it when that was somebody else.
//!
//! These rules are a convention between cooperating parties, not a security boundary. A command
//! runs for an agent only when its caller says so: the control channel without `agent`, the
//! command line, scripts and replayed undo steps all act as the user, and anything that can run
//! commands can say it is the user. What the rules give is a record that tells proposals from
//! decisions and measurements from claims when everybody plays their part.

mod alternatives;
mod checks;
mod compare;
mod context;
mod findings;
mod input;
#[cfg(test)]
mod tests;

use crate::cmd::CommandSpec;

pub use checks::{min_stage, rules};

pub fn specs() -> Vec<CommandSpec> {
    let mut all = context::specs();
    all.extend(findings::specs());
    all.extend(alternatives::specs());
    all
}

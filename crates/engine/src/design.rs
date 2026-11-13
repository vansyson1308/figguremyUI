//! The design context commands (`design.*`): the brief, assumptions, decisions, findings from
//! checks and reviews, and alternatives. The data is [`figcraft_doc::design::DesignContext`].
//!
//! The commands keep apart what was measured, what somebody thinks and what a person said, and
//! who may settle each: a check is settled by running its rule again, an opinion or a person's
//! feedback by a person. An AI agent (a command run through [`crate::Session::execute_as`]) can
//! propose, record and claim a fix; it can't accept its own decision, confirm that feedback is
//! met, or record that an assumption about people was borne out.

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

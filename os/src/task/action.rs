//! Signal actions, taken from the RISC-V tutorial (ch7) verbatim.

use crate::task::{MAX_SIG, SignalFlags};

/// Action for a signal
#[repr(C, align(16))]
#[derive(Debug, Clone, Copy)]
pub struct SignalAction {
    /// the address of the user handler, or 0 for the default action
    pub handler: usize,
    /// signals masked while this handler is running
    pub mask: SignalFlags,
}

impl Default for SignalAction {
    fn default() -> Self {
        Self {
            handler: 0,
            mask: SignalFlags::from_bits(40).unwrap(),
        }
    }
}

/// The actions of a process for all signals
#[derive(Clone)]
pub struct SignalActions {
    /// `table[sig]` is the action for signal `sig`
    pub table: [SignalAction; MAX_SIG + 1],
}

impl Default for SignalActions {
    fn default() -> Self {
        Self {
            table: [SignalAction::default(); MAX_SIG + 1],
        }
    }
}

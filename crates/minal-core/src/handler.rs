//! VT escape sequence handler implementing `vte::Perform`.

use tracing::trace;

/// Handler for VT parser events.
///
/// Implements `vte::Perform` to process escape sequences and update
/// terminal state.
pub struct Handler;

impl vte::Perform for Handler {
    fn print(&mut self, c: char) {
        trace!("print: {:?}", c);
    }

    fn execute(&mut self, byte: u8) {
        trace!("execute: {:#04x}", byte);
    }

    fn hook(&mut self, params: &vte::Params, intermediates: &[u8], ignore: bool, action: char) {
        trace!(
            "hook: params={:?}, intermediates={:?}, ignore={}, action={:?}",
            params, intermediates, ignore, action
        );
    }

    fn put(&mut self, byte: u8) {
        trace!("put: {:#04x}", byte);
    }

    fn unhook(&mut self) {
        trace!("unhook");
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
        trace!(
            "osc_dispatch: params={:?}, bell_terminated={}",
            params, bell_terminated
        );
    }

    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        intermediates: &[u8],
        ignore: bool,
        action: char,
    ) {
        trace!(
            "csi_dispatch: params={:?}, intermediates={:?}, ignore={}, action={:?}",
            params, intermediates, ignore, action
        );
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        trace!(
            "esc_dispatch: intermediates={:?}, ignore={}, byte={:#04x}",
            intermediates, ignore, byte
        );
    }
}

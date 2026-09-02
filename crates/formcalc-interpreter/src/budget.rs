//! The stack budget the evaluator spends when it recurses.
//!
//! Every bound in this crate is *counted* -- expression depth in the parser,
//! frames and evaluation steps in the interpreter -- and a count only protects
//! the stack if each step is known to cost less than the stack has room for.
//! That relation held in release builds (about 780 bytes a level) and broke in
//! debug builds (about 17 KB a level, before the evaluator's match arms were
//! split into their own frames): the same `MAX_EVAL_DEPTH` that refused a
//! hostile form in one profile let it overflow a 2 MB thread in the other.
//!
//! So the evaluator measures the stack it has consumed since it was entered,
//! in bytes, and refuses at [`STACK_BUDGET_BYTES`] regardless of what a level
//! happens to cost in this build. The count remains the semantic bound; this is
//! the physical one, and both must hold.
//!
//! Measuring is pure Rust: the address of a local at entry against the address
//! of a local now. The stack grows downward on every target this crate ships
//! on (x86_64, aarch64, wasm32's shadow stack), and the number it produces is
//! the number the platform charges, whatever the build profile.

/// How much stack evaluation may consume, from the point the interpreter is
/// entered to the deepest frame it reaches.
///
/// Sized against the smallest stack the pipeline runs on: wasm32 has no
/// `std::thread`, so `flatten` runs the whole XFA pipeline inline on the
/// host's stack, and nothing in the tree sets `-zstack-size`, so that stack is
/// the wasm32 default of 1 MiB. Half of it is left for whatever sits above the
/// interpreter -- template parsing, layout, the caller's own frames.
///
/// On native the interpreter runs on the 32 MB thread `flatten` spawns and this
/// budget is far from the edge; it is the wasm32 number that has to be right.
pub const STACK_BUDGET_BYTES: usize = 512 * 1024;

/// Where the stack currently is, as an address.
///
/// Only differences between two readings mean anything, and they mean bytes of
/// stack between the two points. Not inlined, so every reading is taken from a
/// frame of the same shape and the differences stay comparable.
#[inline(never)]
pub fn stack_position() -> usize {
    let probe = 0u8;
    // `black_box` keeps the local on the stack, so its address is a stack
    // address and not a register. The address is used as an integer only;
    // nothing is read through it.
    std::hint::black_box(&probe as *const u8 as usize)
}

/// A stack budget measured from the point it was started.
#[derive(Debug, Clone, Copy)]
pub struct StackBudget {
    base: usize,
    limit: usize,
}

impl StackBudget {
    /// Start measuring here, with `limit` bytes to spend.
    pub fn start(limit: usize) -> Self {
        Self {
            base: stack_position(),
            limit,
        }
    }

    /// Bytes of stack consumed since the budget was started.
    ///
    /// Saturating, so a reading taken above the base (which cannot happen on
    /// a downward-growing stack while the starting frame is live) reads as
    /// nothing spent rather than as everything.
    pub fn used(&self) -> usize {
        self.base.saturating_sub(stack_position())
    }

    /// The limit this budget was started with.
    pub fn limit(&self) -> usize {
        self.limit
    }

    /// Whether more than the limit has been consumed.
    pub fn exhausted(&self) -> bool {
        self.used() > self.limit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A frame deeper must read as more stack used, or nothing here measures.
    #[test]
    fn deeper_frames_read_as_more_stack() {
        #[inline(never)]
        fn descend(budget: &StackBudget, levels: usize, pad: [u8; 256]) -> usize {
            let here = budget.used();
            if levels == 0 {
                std::hint::black_box(pad);
                return here;
            }
            let below = descend(budget, levels - 1, std::hint::black_box(pad));
            assert!(
                below > here,
                "a deeper frame read {below}, not more than {here}"
            );
            below
        }
        let budget = StackBudget::start(STACK_BUDGET_BYTES);
        let deepest = descend(&budget, 32, [0u8; 256]);
        assert!(
            deepest >= 32 * 256,
            "32 frames of 256 bytes read as {deepest}"
        );
        assert!(!budget.exhausted());
    }

    #[test]
    fn a_zero_budget_is_exhausted_one_frame_down() {
        #[inline(never)]
        fn one_frame_down(budget: &StackBudget) -> bool {
            budget.exhausted()
        }
        let budget = StackBudget::start(0);
        assert!(one_frame_down(&budget));
    }
}

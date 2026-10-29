//! Total Commander's selection masks, as Num+ and Num- take them, and the wildcard matcher every
//! mask syntax of the engine compares names with, the lowering of a name for it, and the one
//! reader of mask lists the search and synchronize masks share. All of them live in `rc-ops`
//! (`rc_ops::mask`), which the unpack job shares; this module is their path inside the engine.

pub(crate) use rc_ops::mask::{Mask, Token, lowercase, split_line, wildcard};

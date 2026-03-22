use anchor_lang::prelude::*;

/// Return data interface for hook::on_borrow.
/// Any conforming hook must set_return_data with borsh-serialized bytes matching this layout.
#[derive(AnchorDeserialize)]
pub struct OnBorrowResult {
    pub approved: bool,
    pub interest_bips: u16,
    pub term_seconds: i64,
}

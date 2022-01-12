
/// These are abstrated periphrial backends to make it easy to implemnt new MMIO
/// You can use or combine these backends in observers 

pub mod compare_match_timer;
mod interrupt;
pub use compare_match_timer::CompareMatchTimer;
pub use interrupt::Interrupt;
pub use interrupt::InterruptError;
pub use interrupt::InterruptHandler;
pub use interrupt::InterruptHandlerOverrider;
pub use interrupt::EmptyInterruptHandlerOverrider;

// pub use rscan::{RSCan};
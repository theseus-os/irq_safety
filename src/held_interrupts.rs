// Inspired by Tifflin OS

use core::{
    arch::asm,
    sync::atomic::{compiler_fence, Ordering},
};

/// A handle for frozen interrupts
#[derive(Default)]
pub struct HeldInterrupts(bool);

impl !Send for HeldInterrupts {}

/// Prevent interrupts from firing until the return value is dropped (goes out of scope).
/// After it is dropped, the interrupts are returned to their prior state, not blindly re-enabled.
pub fn hold_interrupts() -> HeldInterrupts {
    let enabled = interrupts_enabled();
    let retval = HeldInterrupts(enabled);
    disable_interrupts();
    // trace!("hold_interrupts(): disabled interrupts, were {}", enabled);
    retval
}

impl Drop for HeldInterrupts {
    fn drop(&mut self) {
        // trace!("hold_interrupts(): enabling interrupts? {}", self.0);
        if self.0 {
            enable_interrupts();
        }
    }
}

// Rust wrappers around the x86-family of interrupt-related instructions.
#[inline(always)]
pub fn enable_interrupts() {
    compiler_fence(Ordering::SeqCst);
    unsafe {
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        asm!("sti", options(nomem, nostack));

        #[cfg(target_arch = "aarch64")]
        // Clear the i and f bits.
        asm!("msr daifclr, #3", options(nomem, nostack, preserves_flags));

        #[cfg(target_arch = "arm")]
        asm!("cpsie i", options(nomem, nostack, preserves_flags));
    }
}

#[inline(always)]
pub fn disable_interrupts() {
    unsafe {
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        asm!("cli", options(nomem, nostack));

        #[cfg(target_arch = "aarch64")]
        // Set the i and f bits.
        asm!("msr daifset, #3", options(nomem, nostack, preserves_flags));

        #[cfg(target_arch = "arm")]
        asm!("cpsid i", options(nomem, nostack, preserves_flags));
    }
    compiler_fence(Ordering::SeqCst);
}

#[inline(always)]
pub fn interrupts_enabled() -> bool {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    unsafe {
        // we only need the lower 16 bits of the eflags/rflags register
        let flags: usize;
        asm!("pushfq; pop {}", out(reg) flags, options(nomem, preserves_flags));
        (flags & 0x0200) != 0
    }

    #[cfg(target_arch = "aarch64")]
    unsafe {
        let daif: usize;
        asm!("mrs {}, daif", out(reg) daif, options(nomem, nostack, preserves_flags));
        // The flags are stored in bits 7-10. We only care about i and f,
        // stored in bits 7 and 8.
        daif >> 6 & 0x3 != 0
    }

    #[cfg(target_arch = "arm")]
    unsafe {
        let primask: u32;
        asm!("mrs {}, primask", out(reg) primask, options(nomem, nostack, preserves_flags));
        primask & (1 << 0) != (1 << 0)
    }
}

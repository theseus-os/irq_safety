// Inspired by Tifflin OS

#[cfg(any(target_arch = "arm"))]
extern crate cortex_m;

#[cfg(any(target_arch = "arm"))]
use self::cortex_m::{interrupt, register};

use core::arch::asm;

/// A handle for frozen interrupts
#[derive(Default)]
pub struct HeldInterrupts(bool);

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
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
        unsafe { asm!("sti", options(nomem, nostack)); }
    }

    #[cfg(any(target_arch = "aarch64"))]
    {
        core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
        unsafe {
            // Clear the i and f bits.
            asm!("msr daifclr, #3", options(nomem, nostack, preserves_flags));
        };
    }

    #[cfg(any(target_arch = "arm"))]
    unsafe { interrupt::enable(); }
}

#[inline(always)]
pub fn disable_interrupts() {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        unsafe { asm!("cli", options(nomem, nostack)) };
        core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
    }

    #[cfg(any(target_arch = "aarch64"))]
    {
        unsafe {
            // Set the i and f bits.
            asm!("msr daifset, #3", options(nomem, nostack, preserves_flags));
        };
        core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
    }

    #[cfg(any(target_arch = "arm"))]
    interrupt::disable();
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

    #[cfg(any(target_arch = "aarch64"))]
    unsafe {
        let daif: usize;
        asm!("mrs {}, daif", out(reg) daif, options(nomem, preserves_flags));
        // The flags are stored in bits 7-10. We only care about i and f,
        // stored in bits 7 and 8.
        daif >> 6 & 0x3 == 0
    }

    #[cfg(any(target_arch = "arm"))]
    register::primask::read().is_active()
}

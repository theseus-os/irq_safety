// Originally inspired by Tifflin OS.

// The assembly blocks intentionally omit the `nomem` option to prevent the
// compiler from reordering memory accesses across the block.
//
// See https://github.com/mkroening/interrupt-ref-cell/issues/5#issuecomment-1753047784.

use core::arch::asm;

/// A guard type for withholding regular interrupts on the current CPU.
///
/// When dropped, interrupts are returned to their prior state rather than
/// just blindly re-enabled. For example, if interrupts were enabled
/// when [`hold_interrupts()`] was invoked, interrupts will be re-enabled
/// when this type is dropped.
#[derive(Default)]
pub struct HeldInterrupts(bool);

impl !Send for HeldInterrupts {}

/// Prevents regular interrupts from occurring until the returned
/// `HeldInterrupts` object is dropped.
///
/// This function only affects *regular* IRQs;
/// it does not affect NMIs or fast interrupts (FIQs on aarch64).
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

/// Unconditionally enables *regular* interrupts (IRQs),
/// not NMIs or fast interrupts (FIQs on aarch64).
///
/// To enable fast interrupts (FIQs) on aarch64,
/// use the [`enable_fast_interrupts()`] interrupts.
#[inline(always)]
pub fn enable_interrupts() {
    unsafe {
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        asm!("sti", options(nostack, preserves_flags));

        #[cfg(target_arch = "aarch64")]
        // Clear the I bit, which is bit 1 of the DAIF bitset.
        asm!("msr daifclr, #2", options(nostack, preserves_flags));

        #[cfg(target_arch = "arm")]
        asm!("cpsie i", options(nostack, preserves_flags));
    }
}

/// Unconditionally disables *regular* interrupts (IRQs),
/// not NMIs or fast interrupts (FIQs on aarch64).
///
/// To disable fast interrupts (FIQs) on aarch64,
/// use the [`disable_fast_interrupts()`] interrupts.
#[inline(always)]
pub fn disable_interrupts() {
    unsafe {
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        asm!("cli", options(nostack, preserves_flags));

        #[cfg(target_arch = "aarch64")]
        // Set the I bit, which is bit 1 of the DAIF bitset.
        asm!("msr daifset, #2", options(nomem, nostack, preserves_flags));

        #[cfg(target_arch = "arm")]
        asm!("cpsid i", options(nostack, preserves_flags));
    }
}

/// Unconditionally enables fast interrupts (FIQs); aarch64-only.
///
/// On aarch64, NMIs are only available as a hardware extension,
/// therefore we only deal with FIQs here, which are widely supported.
#[inline(always)]
#[cfg(target_arch = "aarch64")]
pub fn enable_fast_interrupts() {
    unsafe {
        // Clear the F bit, which is bit 0 of the DAIF bitset.
        asm!("msr daifclr, #1", options(nostack, preserves_flags));
    }
}

/// Unconditionally disables fast interrupts (FIQs); aarch64-only.
///
/// On aarch64, NMIs are only available as a hardware extension,
/// therefore we only deal with FIQs here, which are widely supported.
#[inline(always)]
#[cfg(target_arch = "aarch64")]
pub fn disable_fast_interrupts() {
    unsafe {
        // Clear the F bit, which is bit 0 of the DAIF bitset.
        asm!("msr daifset, #1", options(nostack, preserves_flags));
    }
}

/// Returns whether regular interrupts are enabled on the current CPU.
///
/// This only checks whether *regular* interrupts are enabled,
/// not NMIs or fast interrupts (FIQs on aarch64).
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
        // PSTATE flags of interest are in bits [6:9]; we only care about I, stored in bit 7.
        (daif & (1 << 7)) == 0
    }

    #[cfg(target_arch = "arm")]
    unsafe {
        let primask: u32;
        asm!("mrs {}, primask", out(reg) primask, options(nomem, nostack, preserves_flags));
        primask & (1 << 0) != (1 << 0)
    }
}

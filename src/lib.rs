//! Types for interrupt-safe operations, i.e., avoiding deadlock when
//! sharing data between a regular thread context and an interrupt handler context.
//! 
//! Key types include:
//! * [`HeldInterrupts`]: a guard type that auto-reenables interrupts when dropped,
//!   only if they were originally enabled when the guard was created.
//! * [ MutexIrqSafe`] and [`RwLockIrqSafe`]: spinlock wrappers that use [`spin::Mutex`]
//!   and [`spin::RwLock`] internally to auto-disable interrupts for the duration of 
//!   the lock being held.
//! 
//! Both of these types implement the [`stable_deref_trait::StableDeref`] trait,
//! allowing them to be used with crates like `owning_ref`.

#![no_std]

extern crate spin;
extern crate owning_ref;
extern crate stable_deref_trait;

pub use mutex_irqsafe::*;
pub use rwlock_irqsafe::*;
pub use held_interrupts::*;

mod mutex_irqsafe;
mod rwlock_irqsafe;
mod held_interrupts;

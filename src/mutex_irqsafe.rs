use core::marker::Sync;
use core::ops::{Drop, Deref, DerefMut};
use core::fmt;
use core::option::Option::{self, None, Some};
use core::default::Default;
use core::mem::ManuallyDrop;

use spin::{Mutex, MutexGuard};
use held_interrupts::{HeldInterrupts, hold_interrupts};
use stable_deref_trait::StableDeref;
use owning_ref::{OwningRef, OwningRefMut};

/// This type provides interrupt-safe MUTual EXclusion based on [spin::Mutex].
///
/// # Description
///
/// This structure behaves a lot like a normal MutexIrqSafe. There are some differences:
///
/// - It may be used outside the runtime.
///   - A normal MutexIrqSafe will fail when used without the runtime, this will just lock
///   - When the runtime is present, it will call the deschedule function when appropriate
/// - No lock poisoning. When a fail occurs when the lock is held, no guarantees are made
///
/// When calling rust functions from bare threads, such as C `pthread`s, this lock will be very
/// helpful. In other cases however, you are encouraged to use the locks from the standard
/// library.
///
/// # Simple examples
///
/// ```
/// use irq_safety;
/// let spin_mutex = irq_safety::MutexIrqSafe::new(0);
///
/// // Modify the data
/// {
///     let mut data = spin_mutex.lock();
///     *data = 2;
/// }
///
/// // Read the data
/// let answer =
/// {
///     let data = spin_mutex.lock();
///     *data
/// };
///
/// assert_eq!(answer, 2);
/// ```
///
/// # Thread-safety example
///
/// ```
/// use irq_safety;
/// use std::sync::{Arc, Barrier};
///
/// let numthreads = 1000;
/// let spin_mutex = Arc::new(irq_safety::MutexIrqSafe::new(0));
///
/// // We use a barrier to ensure the readout happens after all writing
/// let barrier = Arc::new(Barrier::new(numthreads + 1));
///
/// for _ in (0..numthreads)
/// {
///     let my_barrier = barrier.clone();
///     let my_lock = spin_mutex.clone();
///     std::thread::spawn(move||
///     {
///         let mut guard = my_lock.lock();
///         *guard += 1;
///
///         // Release the lock to prevent a deadlock
///         drop(guard);
///         my_barrier.wait();
///     });
/// }
///
/// barrier.wait();
///
/// let answer = { *spin_mutex.lock() };
/// assert_eq!(answer, numthreads);
/// ```
pub struct MutexIrqSafe<T: ?Sized>
{
    lock: Mutex<T>,
}

/// A guard to which the protected data can be accessed
///
/// When the guard falls out of scope it will release the lock.
pub struct MutexIrqSafeGuard<'a, T: ?Sized + 'a>
{
    held_irq: ManuallyDrop<HeldInterrupts>,
    guard: ManuallyDrop<MutexGuard<'a, T>>, 
}

// Same unsafe impls as `std::sync::MutexIrqSafe`
unsafe impl<T: ?Sized + Send> Sync for MutexIrqSafe<T> {}
unsafe impl<T: ?Sized + Send> Send for MutexIrqSafe<T> {}

impl<T> MutexIrqSafe<T>
{
    /// Creates a new spinlock wrapping the supplied data.
    ///
    /// May be used statically:
    ///
    /// ```
    /// use irq_safety;
    ///
    /// static MutexIrqSafe: irq_safety::MutexIrqSafe<()> = irq_safety::MutexIrqSafe::new(());
    ///
    /// fn demo() {
    ///     let lock = MutexIrqSafe.lock();
    ///     // do something with lock
    ///     drop(lock);
    /// }
    /// ```
    pub const fn new(user_data: T) -> MutexIrqSafe<T>
    {
        MutexIrqSafe
        {
            lock: Mutex::new(user_data),
        }
    }

    /// Consumes this MutexIrqSafe, returning the underlying data.
    pub fn into_inner(self) -> T {
        self.lock.into_inner()
    }
}

impl<T: ?Sized> MutexIrqSafe<T>
{
    // fn obtain_lock(&self)
    // {
    //     while self.lock.compare_and_swap(false, true, Ordering::Acquire) != false
    //     {
    //         // Wait until the lock looks unlocked before retrying
    //         while self.lock.load(Ordering::Relaxed)
    //         {
    //             cpu_relax();
    //         }
    //     }
    // }

    /// Locks the spinlock and returns a guard.
    ///
    /// The returned value may be dereferenced for data access
    /// and the lock will be dropped when the guard falls out of scope.
    ///
    /// ```
    /// let mylock = irq_safety::MutexIrqSafe::new(0);
    /// {
    ///     let mut data = mylock.lock();
    ///     // The lock is now locked and the data can be accessed
    ///     *data += 1;
    ///     // The lock is implicitly dropped
    /// }
    ///
    /// ```
    pub fn lock(&self) -> MutexIrqSafeGuard<T>
    {
        MutexIrqSafeGuard
        {
            held_irq: ManuallyDrop::new(hold_interrupts()),
            guard: ManuallyDrop::new(self.lock.lock())
        }
    }

    /// Force unlock the spinlock.
    ///
    /// This is *extremely* unsafe if the lock is not held by the current
    /// thread. However, this can be useful in some instances for exposing the
    /// lock to FFI that doesn't know how to deal with RAII.
    ///
    /// If the lock isn't held, this is a no-op.
    pub unsafe fn force_unlock(&self) {
        self.lock.force_unlock()
    }

    /// Tries to lock the MutexIrqSafe. If it is already locked, it will return None. Otherwise it returns
    /// a guard within Some.
    pub fn try_lock(&self) -> Option<MutexIrqSafeGuard<T>>
    {
        let held_irq = ManuallyDrop::new(hold_interrupts());
        match self.lock.try_lock() {
            None => None,
            success => {
                Some(
                    MutexIrqSafeGuard {
                        held_irq,
                        guard: ManuallyDrop::new(success.unwrap()),
                    }
                )
            }
        }
    }

}

impl<T: ?Sized + fmt::Debug> fmt::Debug for MutexIrqSafe<T>
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        match self.lock.try_lock()
        {
            Some(guard) => write!(f, "MutexIrqSafe {{ data: {:?} }}", &*guard),
            None => write!(f, "MutexIrqSafe {{ <locked> }}"),
        }
    }
}

impl<T: ?Sized + Default> Default for MutexIrqSafe<T> {
    fn default() -> MutexIrqSafe<T> {
        MutexIrqSafe::new(Default::default())
    }
}

impl<'a, T: ?Sized> Deref for MutexIrqSafeGuard<'a, T>
{
    type Target = T;

    fn deref<'b>(&'b self) -> &'b T { 
        & *(self.guard) 
    }
}

impl<'a, T: ?Sized> DerefMut for MutexIrqSafeGuard<'a, T>
{
    fn deref_mut<'b>(&'b mut self) -> &'b mut T { 
        &mut *(self.guard)
    }
}


// NOTE: we need explicit calls to .drop() to ensure that HeldInterrupts are not released 
//       until the inner lock is also released.
impl<'a, T: ?Sized> Drop for MutexIrqSafeGuard<'a, T>
{
    /// The dropping of the MutexIrqSafeGuard will release the lock it was created from.
    fn drop(&mut self) {
        unsafe {
            ManuallyDrop::drop(&mut self.guard);
            ManuallyDrop::drop(&mut self.held_irq);
        }
    }
}

// Implement the StableDeref trait for MutexIrqSafe guards, just like it's implemented for Mutex guards
unsafe impl<'a, T: ?Sized> StableDeref for MutexIrqSafeGuard<'a, T> {}

/// Typedef of a owning reference that uses a `MutexIrqSafeGuard` as the owner.
pub type MutexIrqSafeGuardRef<'a, T, U = T> = OwningRef<MutexIrqSafeGuard<'a, T>, U>;
/// Typedef of a mutable owning reference that uses a `MutexIrqSafeGuard` as the owner.
pub type MutexIrqSafeGuardRefMut<'a, T, U = T> = OwningRefMut<MutexIrqSafeGuard<'a, T>, U>;

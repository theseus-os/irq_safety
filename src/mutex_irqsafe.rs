use core::{fmt, ops::{Deref, DerefMut}};
use spin::{Mutex, MutexGuard};
use crate::held_interrupts::{HeldInterrupts, hold_interrupts};
use stable_deref_trait::StableDeref;
use owning_ref::{OwningRef, OwningRefMut};

/// This type provides interrupt-safe MUTual EXclusion based on [spin::Mutex].
///
/// # Description
///
/// This structure behaves a lot like a normal Mutex. There are some differences:
///
/// - It may be used outside the runtime.
///   - A normal Mutex will fail when used without the runtime, this will just lock
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
pub struct MutexIrqSafe<T: ?Sized> {
    lock: Mutex<T>,
}

/// A guard to which the protected data can be accessed
///
/// When the guard falls out of scope it will release the lock.
pub struct MutexIrqSafeGuard<'a, T: ?Sized + 'a> {
    guard: MutexGuard<'a, T>,
    // `_held_irq` will be dropped after `guard`.
    // Rust guarantees that fields are dropped in the order of declaration.
    _held_irq: HeldInterrupts,
}

// Same unsafe impls as `std::sync::MutexIrqSafe`
unsafe impl<T: ?Sized + Send> Sync for MutexIrqSafe<T> {}
unsafe impl<T: ?Sized + Send> Send for MutexIrqSafe<T> {}

impl<T> MutexIrqSafe<T> {
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
    pub const fn new(data: T) -> MutexIrqSafe<T> {
        MutexIrqSafe {
            lock: Mutex::new(data),
        }
    }

    /// Consumes this MutexIrqSafe, returning the underlying data.
    #[inline(always)]
    pub fn into_inner(self) -> T {
        self.lock.into_inner()
    }
}

impl<T: ?Sized> MutexIrqSafe<T> {
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
    #[inline(always)]
    pub fn lock(&self) -> MutexIrqSafeGuard<T> {
        loop {
            match self.try_lock() {
                Some(guard) => return guard,
                _ => {}
            }
        }
    }

    /// Returns `true` if the lock is currently held.
    ///
    /// # Safety
    ///
    /// This function provides no synchronization guarantees and so its result should be considered 'out of date'
    /// the instant it is called. Do not use it for synchronization purposes. However, it may be useful as a heuristic.
    #[inline(always)]
    pub fn is_locked(&self) -> bool {
        self.lock.is_locked()
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
    #[inline(always)]
    pub fn try_lock(&self) -> Option<MutexIrqSafeGuard<T>> {
        if self.lock.is_locked() { return None; }
        let _held_irq = hold_interrupts();
        self.lock.try_lock().map(|guard| MutexIrqSafeGuard {
            guard,
            _held_irq,
        })
    }

    /// Returns a mutable reference to the underlying data.
    ///
    /// Since this call borrows the [`MutexIrqSafe`] mutably, and a mutable reference is guaranteed to be exclusive in Rust,
    /// no actual locking needs to take place -- the mutable borrow statically guarantees no locks exist. As such,
    /// this is a 'zero-cost' operation.
    ///
    /// # Example
    ///
    /// ```
    /// let mut lock = irq_safety::MutexIrqSafe::new(0);
    /// *lock.get_mut() = 10;
    /// assert_eq!(*lock.lock(), 10);
    /// ```
    #[inline(always)]
    pub fn get_mut(&mut self) -> &mut T {
        self.lock.get_mut()
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for MutexIrqSafe<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.lock.try_lock() {
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

impl<'a, T: ?Sized> Deref for MutexIrqSafeGuard<'a, T> {
    type Target = T;

    fn deref<'b>(&'b self) -> &'b T { 
        & *(self.guard) 
    }
}

impl<'a, T: ?Sized> DerefMut for MutexIrqSafeGuard<'a, T> {
    fn deref_mut<'b>(&'b mut self) -> &'b mut T { 
        &mut *(self.guard)
    }
}

// Implement the StableDeref trait for MutexIrqSafe guards, just like it's implemented for Mutex guards
unsafe impl<'a, T: ?Sized> StableDeref for MutexIrqSafeGuard<'a, T> {}

/// Typedef of a owning reference that uses a `MutexIrqSafeGuard` as the owner.
pub type MutexIrqSafeGuardRef<'a, T, U = T> = OwningRef<MutexIrqSafeGuard<'a, T>, U>;
/// Typedef of a mutable owning reference that uses a `MutexIrqSafeGuard` as the owner.
pub type MutexIrqSafeGuardRefMut<'a, T, U = T> = OwningRefMut<MutexIrqSafeGuard<'a, T>, U>;

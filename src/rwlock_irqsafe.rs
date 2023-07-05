use core::{fmt, ops::{Deref, DerefMut}};
use spin::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use crate::held_interrupts::{HeldInterrupts, hold_interrupts};

/// A simple wrapper around a `RwLock` whose guards disable interrupts properly 
pub struct RwLockIrqSafe<T: ?Sized> {
    rwlock: RwLock<T>,
}


/// A guard to which the protected data can be read
///
/// When the guard falls out of scope it will decrement the read count,
/// potentially releasing the lock and potentially re-enabling interrupts.
pub struct RwLockIrqSafeReadGuard<'a, T: 'a + ?Sized> {
    guard: RwLockReadGuard<'a, T>,
    // `_held_irq` will be dropped after `guard`.
    // Rust guarantees that fields are dropped in the order of declaration.
    _held_irq: HeldInterrupts,
}

/// A guard to which the protected data can be written
///
/// When the guard falls out of scope it will release the lock and potentially re-enable interrupts.
pub struct RwLockIrqSafeWriteGuard<'a, T: 'a + ?Sized> {
    guard: RwLockWriteGuard<'a, T>,
    // `_held_irq` will be dropped after `guard`.
    // Rust guarantees that fields are dropped in the order of declaration.
    _held_irq: HeldInterrupts,
}

// Same unsafe impls as `std::sync::RwLock`
unsafe impl<T: ?Sized + Send> Send for RwLockIrqSafe<T> {}
unsafe impl<T: ?Sized + Send + Sync> Sync for RwLockIrqSafe<T> {}


impl<T> RwLockIrqSafe<T> {
    /// Creates a new spinlock wrapping the supplied data.
    ///
    /// May be used statically:
    ///
    /// ```
    /// static RW_LOCK_IRQ_SAFE: RwLockIrqSafe<()> = RwLockIrqSafe::new(());
    ///
    /// fn demo() {
    ///     let lock = RW_LOCK_IRQ_SAFE.read();
    ///     // do something with lock
    ///     drop(lock);
    /// }
    /// ```
    #[inline]
    pub const fn new(data: T) -> RwLockIrqSafe<T> {
        RwLockIrqSafe {
            rwlock: RwLock::new(data),
        }
    }

    /// Consumes this `RwLockIrqSafe`, returning the underlying data.
    pub fn into_inner(self) -> T {
        self.rwlock.into_inner()
    }
}

impl<T: ?Sized> RwLockIrqSafe<T> {
    /// Locks this RwLockIrqSafe with shared read access, blocking the current thread
    /// until it can be acquired.
    ///
    /// The calling thread will be blocked until there are no more writers which
    /// hold the lock. There may be other readers currently inside the lock when
    /// this method returns. This method does not provide any guarantees with
    /// respect to the ordering of whether contentious readers or writers will
    /// acquire the lock first.
    ///
    /// Returns an RAII guard which will release this thread's shared access
    /// once it is dropped, along with restoring interrupts. 
    ///
    /// ```
    /// let mylock = irq_safety::RwLockIrqSafe::new(0);
    /// {
    ///     let mut data = mylock.read();
    ///     // The lock is now locked, interrupts are disabled, and the data can be read
    ///     println!("{}", *data);
    ///     // The lock is dropped and interrupts are restored to their prior state
    /// }
    /// ```
    #[inline]
    pub fn read<'a>(&'a self) -> RwLockIrqSafeReadGuard<'a, T> {
        loop {
            match self.try_read() {
                Some(guard) => return guard,
                _ => {}
            }
        }
    }

    /// Attempt to acquire this lock with shared read access.
    ///
    /// This function will never block and will return immediately if `read`
    /// would otherwise succeed. Returns `Some` of an RAII guard which will
    /// release the shared access of this thread when dropped, or `None` if the
    /// access could not be granted. This method does not provide any
    /// guarantees with respect to the ordering of whether contentious readers
    /// or writers will acquire the lock first.
    ///
    /// ```
    /// let mylock = irq_safety::RwLockIrqSafe::new(0);
    /// {
    ///     match mylock.try_read() {
    ///         Some(data) => {
    ///             // The lock is now locked and the data can be read
    ///             println!("{}", *data);
    ///             // The lock is dropped
    ///         },
    ///         None => (), // no cigar
    ///     };
    /// }
    /// ```
    #[inline]
    pub fn try_read(&self) -> Option<RwLockIrqSafeReadGuard<T>> {
        if self.rwlock.writer_count() > 0 { return None; }
        let _held_irq = hold_interrupts();
        self.rwlock.try_read().map(|guard| RwLockIrqSafeReadGuard {
            guard,
            _held_irq,
        })
    }

    /// Return the number of readers that currently hold the lock (including upgradable readers).
    ///
    /// # Safety
    ///
    /// This function provides no synchronization guarantees and so its result should be considered 'out of date'
    /// the instant it is called. Do not use it for synchronization purposes. However, it may be useful as a heuristic.
    pub fn reader_count(&self) -> usize {
        self.rwlock.reader_count()
    }

    /// Return the number of writers that currently hold the lock.
    ///
    /// Because [`RwLockIrqSafe`] guarantees exclusive mutable access, this function may only return either `0` or `1`.
    ///
    /// # Safety
    ///
    /// This function provides no synchronization guarantees and so its result should be considered 'out of date'
    /// the instant it is called. Do not use it for synchronization purposes. However, it may be useful as a heuristic.
    pub fn writer_count(&self) -> usize {
        self.rwlock.writer_count()
    }

    /// Force decrement the reader count.
    ///
    /// This is *extremely* unsafe if there are outstanding `RwLockReadGuard`s
    /// live, or if called more times than `read` has been called, but can be
    /// useful in FFI contexts where the caller doesn't know how to deal with
    /// RAII.
    pub unsafe fn force_read_decrement(&self) {
        self.rwlock.force_read_decrement();
    }

    /// Force unlock exclusive write access.
    ///
    /// This is *extremely* unsafe if there are outstanding `RwLockWriteGuard`s
    /// live, or if called when there are current readers, but can be useful in
    /// FFI contexts where the caller doesn't know how to deal with RAII.
    pub unsafe fn force_write_unlock(&self) {
        self.rwlock.force_write_unlock();
    }

    /// Lock this rwlock with exclusive write access, blocking the current
    /// thread until it can be acquired.
    ///
    /// This function will not return while other writers or other readers
    /// currently have access to the lock.
    ///
    /// Returns an RAII guard which will drop the write access of this rwlock
    /// when dropped.
    ///
    /// ```
    /// let mylock = irq_safety::RwLockIrqSafe::new(0);
    /// {
    ///     let mut data = mylock.write();
    ///     // The lock is now locked and the data can be written
    ///     *data += 1;
    ///     // The lock is dropped
    /// }
    /// ```
    #[inline]
    pub fn write<'a>(&'a self) -> RwLockIrqSafeWriteGuard<'a, T> {
        loop {
            match self.try_write() {
                Some(guard) => return guard,
                _ => {}
            }
        }
    }

    /// Attempt to lock this rwlock with exclusive write access.
    ///
    /// This function does not ever block, and it will return `None` if a call
    /// to `write` would otherwise block. If successful, an RAII guard is
    /// returned.
    ///
    /// ```
    /// let mylock = irq_safety::RwLockIrqSafe::new(0);
    /// {
    ///     match mylock.try_write() {
    ///         Some(mut data) => {
    ///             // The lock is now locked and the data can be written
    ///             *data += 1;
    ///             // The lock is implicitly dropped
    ///         },
    ///         None => (), // no cigar
    ///     };
    /// }
    /// ```
    #[inline]
    pub fn try_write(&self) -> Option<RwLockIrqSafeWriteGuard<T>> {
        if self.rwlock.writer_count() > 0 || self.rwlock.reader_count() > 0 {
            return None;
        }
        let _held_irq = hold_interrupts();
        self.rwlock.try_write().map(|guard| RwLockIrqSafeWriteGuard {
            guard,
            _held_irq,
        })
    }

    /// Returns a mutable reference to the underlying data.
    ///
    /// Since this call borrows the [`RwLockIrqSafe`] mutably, and a mutable reference is guaranteed to be exclusive in Rust,
    /// no actual locking needs to take place -- the mutable borrow statically guarantees no locks exist. As such,
    /// this is a 'zero-cost' operation.
    ///
    /// # Example
    ///
    /// ```
    /// let mut lock = irq_safety::RwLockIrqSafe::new(0);
    /// *lock.get_mut() = 10;
    /// assert_eq!(*lock.lock(), 10);
    /// ```
    #[inline(always)]
    pub fn get_mut(&mut self) -> &mut T {
        self.rwlock.get_mut()
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for RwLockIrqSafe<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.rwlock.try_read() {
            Some(guard) => write!(f, "RwLockIrqSafe {{ data: {:?} }}", &*guard),
            None => write!(f, "RwLockIrqSafe {{ <locked> }}"),
        }
    }
}

impl<T: ?Sized + Default> Default for RwLockIrqSafe<T> {
    fn default() -> RwLockIrqSafe<T> {
        RwLockIrqSafe::new(Default::default())
    }
}

impl<'rwlock, T: ?Sized> Deref for RwLockIrqSafeReadGuard<'rwlock, T> {
    type Target = T;

    fn deref(&self) -> &T { 
       & *(self.guard) 
    }
}

impl<'rwlock, T: ?Sized> Deref for RwLockIrqSafeWriteGuard<'rwlock, T> {
    type Target = T;

    fn deref(&self) -> &T { 
        & *(self.guard)
    }
}

impl<'rwlock, T: ?Sized> DerefMut for RwLockIrqSafeWriteGuard<'rwlock, T> {
    fn deref_mut(&mut self) -> &mut T { 
        &mut *(self.guard)
    }
}

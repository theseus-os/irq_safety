# irq_safety: Interrupt-safe spinlock Mutex/RwLock

Irq-safe locking via Mutex and RwLock. 

Offers identical behavior to the regular `spin` crate's Mutex and RwLock,
with the added behavior of holding interrupts for the duration of the Mutex guard. 

When the lock guard is dropped (falls out of scope), interrupts are re-enabled 
if and only if they were enabled when the lock was obtained. 

Also provides a interrupt "holding" feature without locking, see the `HeldInterrupts` type. 

This crate is designed for `no_std` usage within an OS kernel or in an embedded context. 

Supported architectures:
* `x86`
* `x86_64`
* `aarch64`
* `arm`

We welcome contributions from anyone, especially for new architectures. 

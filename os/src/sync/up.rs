//! Interior mutability primitives backed by a spinlock.
//!
//! On x86-64 the kernel runs with IF cleared (interrupts only happen in user
//! mode), so while the kernel holds one of these locks it cannot be preempted
//! on the same core; another core spins with `pause` until the lock is
//! released (Intel SDM: PAUSE improves spin-wait performance).
//!
//! In order to get mutable reference of inner data, call `exclusive_access`.

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};

/// A spinlock-protected value.
pub struct UPSafeCell<T> {
    /// true while the lock is held
    locked: AtomicBool,
    /// inner data
    data: UnsafeCell<T>,
}

// The lock provides the synchronization; T only needs to be Send.
unsafe impl<T: Send> Sync for UPSafeCell<T> {}

impl<T> UPSafeCell<T> {
    /// Create a new `UPSafeCell`.
    pub unsafe fn new(value: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            data: UnsafeCell::new(value),
        }
    }
    /// Acquire the spinlock and get a mutable reference to the inner data.
    pub fn exclusive_access(&self) -> SpinLockGuard<'_, T> {
        while self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        SpinLockGuard {
            lock: self,
            data: unsafe { &mut *self.data.get() },
        }
    }
}

/// A mutable guard that releases the spinlock on drop.
pub struct SpinLockGuard<'a, T> {
    lock: &'a UPSafeCell<T>,
    data: &'a mut T,
}

impl<T> Deref for SpinLockGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.data
    }
}

impl<T> DerefMut for SpinLockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.data
    }
}

impl<T> Drop for SpinLockGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
    }
}

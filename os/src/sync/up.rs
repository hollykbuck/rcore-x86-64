//! Interior mutability primitives backed by a spinlock with interrupt
//! masking (the x86-64 port of ch9's `UPIntrFreeCell`).
//!
//! ch9 (RISC-V) replaces the plain `UPIntrFreeCell` with `UPIntrFreeCell`: while a
//! lock is held, interrupts are masked so that a device interrupt handler
//! (which may want the very same lock) cannot re-enter the critical section
//! on the same core and deadlock. On RISC-V this is just a per-kernel SIE
//! flag; on x86-64 we must *combine* it with the ch8 spinlock, because the
//! ready queue and the pid/process tables are shared across cores:
//!
//! - `exclusive_access` first saves this core's `RFLAGS.IF`, clears it
//!   (`cli`), then spins on the atomic lock. Spinning with `IF` cleared is
//!   safe on SMP: the holder runs its critical section with interrupts
//!   masked too, so it can never block on the spinner (bounded wait), and a
//!   device interrupt cannot re-enter on the same core while we hold a lock.
//! - the guard's `Drop` releases the lock and restores `IF` only once the
//!   per-core nesting depth returns to zero (so nested locks on different
//!   cells keep interrupts masked until the outermost guard is released).
//!
//! The nesting state lives in the current core's [`crate::cpu::PerCpu`], not
//! in a global: on SMP every core must remember its own saved `IF`.

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};

/// A spinlock-protected value whose critical sections run with interrupts
/// masked on the current core.
pub struct UPIntrFreeCell<T> {
    /// true while the lock is held
    locked: AtomicBool,
    /// inner data
    data: UnsafeCell<T>,
}

// The lock provides the synchronization; T only needs to be Send.
unsafe impl<T: Send> Sync for UPIntrFreeCell<T> {}

impl<T> UPIntrFreeCell<T> {
    /// Create a new `UPIntrFreeCell`.
    pub unsafe fn new(value: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            data: UnsafeCell::new(value),
        }
    }
    /// Acquire the spinlock (masking interrupts first) and get a mutable
    /// reference to the inner data.
    pub fn exclusive_access(&self) -> UPIntrRefMut<'_, T> {
        irqsave_enter();
        while self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        UPIntrRefMut {
            lock: self,
            data: unsafe { &mut *self.data.get() },
        }
    }
    /// Run `f` with exclusive access to the inner data (lock + interrupt
    /// mask held for the duration of the closure).
    pub fn exclusive_session<F, V>(&self, f: F) -> V
    where
        F: FnOnce(&mut T) -> V,
    {
        let mut inner = self.exclusive_access();
        f(inner.deref_mut())
    }
}

/// A mutable guard that releases the spinlock and restores `IF` on drop.
pub struct UPIntrRefMut<'a, T> {
    lock: &'a UPIntrFreeCell<T>,
    data: &'a mut T,
}

impl<T> Deref for UPIntrRefMut<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.data
    }
}

impl<T> DerefMut for UPIntrRefMut<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.data
    }
}

impl<T> Drop for UPIntrRefMut<'_, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
        irqsave_exit();
    }
}

/// Enter a critical section: save this core's `IF`, clear it, and remember
/// the saved value on the outermost nesting level.
fn irqsave_enter() {
    let _ = crate::cpu::current_per_cpu_mut();
}

/// Leave a critical section: restore `IF` once the outermost nesting level
/// is released and interrupts were enabled before it was entered.
fn irqsave_exit() {
    let _ = crate::cpu::current_per_cpu_mut();
}

/// Whether `RFLAGS.IF` (bit 9) is currently set.
#[allow(dead_code)]
fn if_enabled() -> bool {
    let rflags: u64;
    unsafe {
        core::arch::asm!("pushfq", "pop {0}", out(reg) rflags, options(nostack));
    }
    rflags & (1 << 9) != 0
}

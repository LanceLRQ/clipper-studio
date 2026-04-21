//! Poison-safe wrappers around `std::sync::{Mutex, RwLock}`.
//!
//! A panic inside a lock-holding thread marks the lock as poisoned; subsequent
//! `.lock() / .read() / .write()` return `Err(PoisonError)`. Calling `.unwrap()`
//! in that case produces a secondary panic, which — in a long-running Tauri
//! backend — tends to cascade into a full app crash.
//!
//! In our use, the data guarded by these locks is internally consistent even
//! when a prior writer panicked (paths, registries, queues). So recovering the
//! inner guard and logging a warning is strictly better than crashing.
//!
//! Use `.lock_safe() / .read_safe() / .write_safe()` instead of the raw
//! `.unwrap()` pattern.

use std::sync::{Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};

pub trait MutexExt<T: ?Sized> {
    /// Lock the mutex, recovering the inner guard even if the lock is poisoned.
    /// A poisoned lock emits a `tracing::warn!` once per acquisition.
    fn lock_safe(&self) -> MutexGuard<'_, T>;
}

impl<T: ?Sized> MutexExt<T> for Mutex<T> {
    fn lock_safe(&self) -> MutexGuard<'_, T> {
        match self.lock() {
            Ok(g) => g,
            Err(poisoned) => {
                tracing::warn!("Mutex poisoned, recovering inner guard (a prior holder panicked)");
                poisoned.into_inner()
            }
        }
    }
}

pub trait RwLockExt<T: ?Sized> {
    /// Acquire a read guard, recovering if the lock was poisoned.
    fn read_safe(&self) -> RwLockReadGuard<'_, T>;
    /// Acquire a write guard, recovering if the lock was poisoned.
    fn write_safe(&self) -> RwLockWriteGuard<'_, T>;
}

impl<T: ?Sized> RwLockExt<T> for RwLock<T> {
    fn read_safe(&self) -> RwLockReadGuard<'_, T> {
        match self.read() {
            Ok(g) => g,
            Err(poisoned) => {
                tracing::warn!("RwLock read poisoned, recovering inner guard");
                poisoned.into_inner()
            }
        }
    }

    fn write_safe(&self) -> RwLockWriteGuard<'_, T> {
        match self.write() {
            Ok(g) => g,
            Err(poisoned) => {
                tracing::warn!("RwLock write poisoned, recovering inner guard");
                poisoned.into_inner()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn mutex_recovers_from_poison() {
        let m = Arc::new(Mutex::new(42_i32));
        let m2 = m.clone();
        let _ = thread::spawn(move || {
            let _g = m2.lock().unwrap();
            panic!("poison");
        })
        .join();
        // Raw .lock() would be Err
        assert!(m.lock().is_err());
        // lock_safe recovers
        let g = m.lock_safe();
        assert_eq!(*g, 42);
    }

    #[test]
    fn rwlock_read_recovers_from_poison() {
        let r = Arc::new(RwLock::new(7_i32));
        let r2 = r.clone();
        let _ = thread::spawn(move || {
            let _g = r2.write().unwrap();
            panic!("poison");
        })
        .join();
        assert!(r.read().is_err());
        assert_eq!(*r.read_safe(), 7);
    }

    #[test]
    fn healthy_locks_behave_normally() {
        let m = Mutex::new(1_i32);
        *m.lock_safe() += 1;
        assert_eq!(*m.lock_safe(), 2);

        let r = RwLock::new("hi".to_string());
        r.write_safe().push_str("!");
        assert_eq!(&*r.read_safe(), "hi!");
    }
}

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Traits for shared data objects
//!
//! These traits are used for "view widgets", enabling views (and editing) over
//! shared data.

use kas::event::{Manager, UpdateHandle};
#[allow(unused)] // doc links
use std::cell::RefCell;
use std::fmt::Debug;
use std::ops::{Deref, DerefMut};

/// Shared data which may notify of updates
pub trait SharedData: Debug {
    /// Get an update handle, if any is used to notify of updates
    ///
    /// If the data supports updates through shared references (e.g. via an
    /// internal `RefCell`), then it should have an `UpdateHandle` for notifying
    /// other users of the data of the update, and return that here.
    /// Otherwise, this may simply return `None`.
    ///
    /// Users registering for updates on this handle should, if possible, also
    /// call [`SharedDataRec::enable_recursive_updates`].
    fn update_handle(&self) -> Option<UpdateHandle>;

    /// Update self from an update handle
    ///
    /// Data views which are themselves dependent on other shared data should
    /// register themselves for update via [`Manager::update_shared_data`].
    fn update_self(&self) -> Option<UpdateHandle> {
        None
    }
}

/// Extension over [`SharedData`] enabling recursive updating
pub trait SharedDataRec: SharedData {
    /// Enable recursive updates on this object
    ///
    /// Some data objects (e.g. filters) are themselves dependent on another
    /// data object; this method allows such objects to register for updates on
    /// the underlying object. It should be called by any view over the data.
    ///
    /// The default implementation does nothing.
    fn enable_recursive_updates(&self, mgr: &mut Manager) {
        let _ = mgr;
    }
}

/// Trait for viewable single data items
// Note: we require Debug + 'static to allow widgets using this to implement
// WidgetCore, which requires Debug + Any.
pub trait SingleData: SharedDataRec {
    type Item: Clone;

    // TODO(gat): add get<'a>(&self) -> Self::ItemRef<'a> and get_mut

    /// Get data (clone)
    fn get_cloned(&self) -> Self::Item;

    /// Update data, if supported
    ///
    /// This is optional and required only to support data updates through view
    /// widgets. If implemented, then [`SharedData::update_handle`] should
    /// return a copy of the same update handle.
    ///
    /// Returns an [`UpdateHandle`] if an update occurred. Returns `None` if
    /// updates are unsupported.
    ///
    /// This method takes only `&self`, thus some mechanism such as [`RefCell`]
    /// is required to obtain `&mut` and lower to [`SingleDataMut::set`]. The
    /// provider of this lowering should also provide an [`UpdateHandle`].
    fn update(&self, value: Self::Item) -> Option<UpdateHandle>;
}

/// Trait for writable single data items
pub trait SingleDataMut: SingleData {
    /// Set data, given a mutable (unique) reference
    ///
    /// It can be assumed that no synchronisation is required when a mutable
    /// reference can be obtained.
    fn set(&mut self, value: Self::Item);
}

/// Trait for viewable data lists
pub trait ListData: SharedDataRec {
    /// Key type
    type Key: Clone + Debug + PartialEq + Eq;

    /// Item type
    type Item: Clone;

    /// Number of data items available
    ///
    /// Note: users may assume this is `O(1)`.
    fn len(&self) -> usize;

    // TODO(gat): add get<'a>(&self) -> Self::ItemRef<'a> and get_mut

    /// Check whether a key has data
    fn contains_key(&self, key: &Self::Key) -> bool;

    /// Get data by key (clone)
    fn get_cloned(&self, key: &Self::Key) -> Option<Self::Item>;

    /// Update data, if supported
    ///
    /// This is optional and required only to support data updates through view
    /// widgets. If implemented, then [`SharedData::update_handle`] should
    /// return a copy of the same update handle.
    ///
    /// Returns an [`UpdateHandle`] if an update occurred. Returns `None` if
    /// updates are unsupported.
    ///
    /// This method takes only `&self`, thus some mechanism such as [`RefCell`]
    /// is required to obtain `&mut` and lower to [`ListDataMut::set`]. The
    /// provider of this lowering should also provide an [`UpdateHandle`].
    fn update(&self, key: &Self::Key, value: Self::Item) -> Option<UpdateHandle>;

    // TODO(gat): replace with an iterator
    /// Iterate over (key, value) pairs as a vec
    ///
    /// The result will be in deterministic implementation-defined order, with
    /// a length of `max(limit, data_len)` where `data_len` is the number of
    /// items available.
    fn iter_vec(&self, limit: usize) -> Vec<(Self::Key, Self::Item)> {
        self.iter_vec_from(0, limit)
    }

    /// Iterate over (key, value) pairs as a vec
    ///
    /// The result is the same as `self.iter_vec(start + limit).skip(start)`.
    fn iter_vec_from(&self, start: usize, limit: usize) -> Vec<(Self::Key, Self::Item)>;
}

/// Trait for writable data lists
pub trait ListDataMut: ListData {
    /// Set data for an existing key
    fn set(&mut self, key: &Self::Key, item: Self::Item);
}

impl<T: Debug> SharedData for [T] {
    fn update_handle(&self) -> Option<UpdateHandle> {
        None
    }
}
impl<T: Debug> SharedDataRec for [T] {}
impl<T: Clone + Debug> ListData for [T] {
    type Key = usize;
    type Item = T;

    fn len(&self) -> usize {
        (*self).len()
    }

    fn contains_key(&self, key: &Self::Key) -> bool {
        *key < self.len()
    }

    fn get_cloned(&self, key: &usize) -> Option<Self::Item> {
        self.get(*key).cloned()
    }

    fn update(&self, _: &Self::Key, _: Self::Item) -> Option<UpdateHandle> {
        // Note: plain [T] does not support update, but SharedRc<[T]> does.
        None
    }

    fn iter_vec(&self, limit: usize) -> Vec<(Self::Key, Self::Item)> {
        self.iter().cloned().enumerate().take(limit).collect()
    }

    fn iter_vec_from(&self, start: usize, limit: usize) -> Vec<(Self::Key, Self::Item)> {
        self.iter()
            .cloned()
            .enumerate()
            .skip(start)
            .take(limit)
            .collect()
    }
}
impl<T: Clone + Debug> ListDataMut for [T] {
    fn set(&mut self, key: &Self::Key, item: Self::Item) {
        self[*key] = item;
    }
}

impl<K: Ord + Eq + Clone + Debug, T: Clone + Debug> SharedData
    for std::collections::BTreeMap<K, T>
{
    fn update_handle(&self) -> Option<UpdateHandle> {
        None
    }
}
impl<K: Ord + Eq + Clone + Debug, T: Clone + Debug> SharedDataRec
    for std::collections::BTreeMap<K, T>
{
}
impl<K: Ord + Eq + Clone + Debug, T: Clone + Debug> ListData for std::collections::BTreeMap<K, T> {
    type Key = K;
    type Item = T;

    fn len(&self) -> usize {
        (*self).len()
    }

    fn contains_key(&self, key: &Self::Key) -> bool {
        (*self).contains_key(key)
    }

    fn get_cloned(&self, key: &Self::Key) -> Option<Self::Item> {
        (*self).get(key).cloned()
    }

    fn update(&self, _: &Self::Key, _: Self::Item) -> Option<UpdateHandle> {
        // Note: plain BTreeMap does not support update, but SharedRc<..> does.
        None
    }

    fn iter_vec(&self, limit: usize) -> Vec<(Self::Key, Self::Item)> {
        self.iter()
            .take(limit)
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    fn iter_vec_from(&self, start: usize, limit: usize) -> Vec<(Self::Key, Self::Item)> {
        self.iter()
            .skip(start)
            .take(limit)
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }
}

// TODO(spec): implement using Deref; for now can't since it "might" conflict
// with a RefCell impl on a derived type downstream, according to the solver.
// impl<T: Deref + Debug> SingleData for T
// where
//     <T as Deref>::Target: SingleData,
macro_rules! impl_via_deref {
    ($t: ident: $derived:ty) => {
        impl<$t: SharedData + ?Sized> SharedData for $derived {
            fn update_handle(&self) -> Option<UpdateHandle> {
                self.deref().update_handle()
            }
            fn update_self(&self) -> Option<UpdateHandle> {
                self.deref().update_self()
            }
       }
        impl<$t: SharedDataRec + ?Sized> SharedDataRec for $derived {
            fn enable_recursive_updates(&self, mgr: &mut Manager) {
                self.deref().enable_recursive_updates(mgr);
            }
        }

        impl<$t: SingleData + ?Sized> SingleData for $derived {
            type Item = $t::Item;
            fn get_cloned(&self) -> Self::Item {
                self.deref().get_cloned()
            }
            fn update(&self, value: Self::Item) -> Option<UpdateHandle> {
                self.deref().update(value)
            }
        }

        impl<$t: ListData + ?Sized> ListData for $derived {
            type Key = $t::Key;
            type Item = $t::Item;

            fn len(&self) -> usize {
                self.deref().len()
            }
            fn contains_key(&self, key: &Self::Key) -> bool {
                self.deref().contains_key(key)
            }
            fn get_cloned(&self, key: &Self::Key) -> Option<Self::Item> {
                self.deref().get_cloned(key)
            }

            fn update(&self, key: &Self::Key, value: Self::Item) -> Option<UpdateHandle> {
                self.deref().update(key, value)
            }

            fn iter_vec(&self, limit: usize) -> Vec<(Self::Key, Self::Item)> {
                self.deref().iter_vec(limit)
            }
            fn iter_vec_from(&self, start: usize, limit: usize) -> Vec<(Self::Key, Self::Item)> {
                self.deref().iter_vec_from(start, limit)
            }
        }
    };
    ($t: ident: $derived:ty, $($dd:ty),+) => {
        impl_via_deref!($t: $derived);
        impl_via_deref!($t: $($dd),+);
    };
}
impl_via_deref!(T: &T, &mut T);
impl_via_deref!(T: std::rc::Rc<T>, std::sync::Arc<T>, Box<T>);

macro_rules! impl_via_deref_mut {
    ($t: ident: $derived:ty) => {
        impl<$t: SingleDataMut + ?Sized> SingleDataMut for $derived {
            fn set(&mut self, value: Self::Item) {
                self.deref_mut().set(value)
            }
        }
        impl<$t: ListDataMut + ?Sized> ListDataMut for $derived {
            fn set(&mut self, key: &Self::Key, item: Self::Item) {
                self.deref_mut().set(key, item)
            }
        }
    };
    ($t: ident: $derived:ty, $($dd:ty),+) => {
        impl_via_deref_mut!($t: $derived);
        impl_via_deref_mut!($t: $($dd),+);
    };
}
impl_via_deref_mut!(T: &mut T, Box<T>);

//! Runtime trait implementation.

use crate::ValueData;
use std::fmt;
use uuid::Uuid;

/// A type-description of a trait.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct TraitType {
    pub id: Uuid,
    pub data: Vec<u8>,
}

/// An implementation of a trait.
pub struct TraitImpl {
    tt: TraitType,
    data: ValueData,
}

/// A runtime trait.
pub trait Trait<'a> {
    type Impl: Sync;

    fn trait_type() -> TraitType;

    fn create(imp: &'a Self::Impl) -> Self;
}

pub struct TraitRef<'a, T>(&'a T);

impl<'a, T: Trait<'a> + Sync> Trait<'a> for TraitRef<'a, T> {
    type Impl = T;

    fn trait_type() -> TraitType {
        T::trait_type()
    }

    fn create(imp: &'a Self::Impl) -> Self {
        TraitRef(imp)
    }
}

impl<'a, T> std::ops::Deref for TraitRef<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl TraitType {
    /// Create a new trait type with the given id.
    pub fn new(id: Uuid) -> Self {
        Self::with_data(id, Default::default())
    }

    /// Create a new trait type with the given id and data.
    pub fn with_data(id: Uuid, data: Vec<u8>) -> Self {
        TraitType { id, data }
    }
}

impl TraitImpl {
    pub fn new<T>(tt: TraitType, imp: T) -> Self {
        Self {
            tt,
            data: ValueData::new(imp),
        }
    }

    pub fn for_trait<'a, T: Trait<'a>>(v: T::Impl) -> Self {
        Self {
            tt: T::trait_type(),
            data: ValueData::new(v),
        }
    }

    pub unsafe fn as_ref<T: Sync>(&self) -> &T {
        self.data.as_ref()
    }
}

impl fmt::Debug for TraitImpl {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("TraitImpl").field("tt", &self.tt).finish()
    }
}

impl PartialEq for TraitImpl {
    fn eq(&self, other: &Self) -> bool {
        &self.tt == &other.tt
    }
}

impl Eq for TraitImpl {}

impl std::hash::Hash for TraitImpl {
    fn hash<H: std::hash::Hasher>(&self, h: &mut H) {
        self.tt.hash(h)
    }
}

impl std::borrow::Borrow<TraitType> for TraitImpl {
    fn borrow(&self) -> &TraitType {
        &self.tt
    }
}

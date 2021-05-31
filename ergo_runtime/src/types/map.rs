//! The Map type.

use crate as ergo_runtime;
use crate::abi_stable::{bst::BstMap, type_erase::Erased, StableAbi};
use crate::metadata::Doc;
use crate::traits;
use crate::type_system::{ergo_traits_fn, ErgoType};
use crate::{depends, Dependencies, Source, TypedValue, Value};
use bincode;
use std::collections::BTreeMap;

/// Script map type.
#[derive(Clone, Debug, ErgoType, PartialEq, StableAbi)]
#[repr(C)]
pub struct Map(pub BstMap<Source<Value>, Source<Value>>);

impl From<&'_ Map> for Dependencies {
    fn from(m: &'_ Map) -> Self {
        depends![Map::ergo_type(), ^m.0.iter().map(|(k, v)| depends![**k, **v])]
    }
}

impl From<Map> for TypedValue<Map> {
    fn from(m: Map) -> Self {
        let doc = format!("map with {} entries", m.0.len());
        let mut v = Self::constant(m);
        Doc::set_string(&mut v, doc);
        v
    }
}

impl From<Map> for super::Iter {
    fn from(v: Map) -> Self {
        super::Iter::from_iter(
            v.0.into_iter()
                .map(|(key, value)| key.source().with(super::MapEntry { key, value }.into())),
        )
    }
}

impl traits::NestedValues for Map {
    fn nested_values(&self) -> Vec<&Value> {
        self.0
            .iter()
            .map(|(k, v)| vec![&**k, &**v])
            .flatten()
            .collect()
    }
    fn nested_values_mut(&mut self) -> Vec<&mut Value> {
        self.0
            .iter_mut()
            .map(|(k, v)| vec![&mut **k, &mut **v])
            .flatten()
            .collect()
    }
}

ergo_traits_fn! {
    impl traits::Display for Map {
        async fn fmt(&self, f: &mut traits::Formatter) -> crate::error::RResult<()> {
            async move {
                let mut iter = self.0.iter();
                write!(f, "{{")?;
                if let Some((k,v)) = iter.next() {
                    traits::display(CONTEXT, k.as_ref().unwrap().clone(), f).await?;
                    write!(f, " = ")?;
                    traits::display(CONTEXT, v.as_ref().unwrap().clone(), f).await?;
                }

                for (k,v) in iter {
                    write!(f, ", ")?;
                    traits::display(CONTEXT, k.as_ref().unwrap().clone(), f).await?;
                    write!(f, " = ")?;
                    traits::display(CONTEXT, v.as_ref().unwrap().clone(), f).await?;
                }
                write!(f, "}}")?;
                Ok(())
            }.await.into()
        }
    }

    traits::IntoTyped::<super::Iter>::add_impl::<Map>(traits);

    traits::Nested::add_impl::<Map>(traits);

    impl traits::Stored for Map {
        async fn put(&self, stored_ctx: &traits::StoredContext, item: crate::context::ItemContent) -> crate::RResult<()> {
            async move {
                let mut ids: BTreeMap<u128, u128> = BTreeMap::new();
                for (k, v) in self.0.iter() {
                    let k = k.clone().unwrap();
                    let v = v.clone().unwrap();
                    ids.insert(k.id(), v.id());
                    stored_ctx.write_to_store(CONTEXT, k).await?;
                    stored_ctx.write_to_store(CONTEXT, v).await?;
                }
                Ok(bincode::serialize_into(item, &ids)?)
            }.await.into()
        }

        async fn get(stored_ctx: &traits::StoredContext, item: crate::context::ItemContent) -> crate::RResult<Erased> {
            async move {
                let ids: BTreeMap<u128, u128> = bincode::deserialize_from(item)?;
                let mut vals = BstMap::new();
                for (k_id, v_id) in ids {
                    vals.insert(
                        Source::stored(stored_ctx.read_from_store(CONTEXT, k_id).await?),
                        Source::stored(stored_ctx.read_from_store(CONTEXT, v_id).await?),
                    );
                }
                Ok(Erased::new(Map(vals)))
            }.await.into()
        }
    }

    impl traits::Bind for Map {
        async fn bind(&self, arg: Source<Value>) -> Value {
            let (source, mut arg) = arg.take();

            crate::try_result!(CONTEXT.eval(&mut arg).await);

            crate::value::match_value! { arg,
                super::Index(ind) => {
                    let index = ind.unwrap();

                    match self.0.get(&index).cloned() {
                        Some(v) => v.unwrap(),
                        None => super::Unset.into(),
                    }
                },
                Map(map) => {
                    crate::try_result!(traits::bind_map(CONTEXT, self.0.clone(), source.clone().with(map.clone()), false).await);
                    super::Unit.into()
                },
                v => traits::bind_error(CONTEXT, source.with(v)).into()
            }
        }
    }

    crate::ergo_type_name!(traits, Map);
    traits::ValueByContent::add_nested_impl::<Map>(traits);
}
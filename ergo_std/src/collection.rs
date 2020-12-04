//! Functions over collections.

use abi_stable::std_types::RVec;
use ergo_runtime::{ergo_function, types, ContextExt, FunctionArguments};
use futures::future::{ok, FutureExt, TryFutureExt};
use grease::{depends, make_value, match_value, value::Value};

pub fn module() -> Value {
    crate::grease_string_map! {
        "A map of map and array manipulation functions:"
        "entries": "Get the entries in a map as an array of {key,value}." = entries_fn(),
        "fold": "Fold over the values in an array from first to last." = fold_fn(),
        "get": "Get a key's value from a map, returning unit if it does not exist." = get_fn(),
        "has": "Check whether a map contains a key." = has_fn(),
        "map": "Map a function over the values in an array." = map_fn()
    }
}

fn fold_fn() -> Value {
    ergo_function!(std::collection::fold, |ctx| {
        let func = ctx.args.next().ok_or("fold function not provided")?;
        let orig = ctx.args.next().ok_or("fold base value not provided")?;
        let vals = ctx.args.next().ok_or("fold values not provided")?;

        ctx.unused_arguments()?;

        let func = ctx.source_value_as::<types::Function>(func);
        let func = func.await?;

        let vals = ctx.source_value_as::<types::Array>(vals);
        let vals = vals.await?;

        let deps = depends![*func, *orig, *vals];

        let task = ctx.task.clone();
        let func = func.map(|v| types::Portable::portable(v, ctx));
        Value::dyn_new(
            async move {
                let (func_source, func) = func.take();
                let (vals_source, vals) = vals.take();
                let (func, vals) = task.join(func, vals).await?;
                let val = vals.owned().0.into_iter().fold(ok(orig).boxed(), |acc, v| {
                    let f = &func;
                    let src = &vals_source;
                    let fsrc = &func_source;
                    acc.and_then(move |accv| {
                        f.call(
                            FunctionArguments::positional(vec![accv, src.clone().with(v)]),
                            fsrc.clone(),
                        )
                    })
                    .boxed()
                });

                Ok(val.await?.unwrap().into_any_value())
            },
            deps,
        )
    })
    .into()
}

fn map_fn() -> Value {
    ergo_function!(std::collection::map, |ctx| {
        let func = ctx.args.next().ok_or("map function not provided")?;
        let arr = ctx.args.next().ok_or("map array not provided")?;

        ctx.unused_arguments()?;

        let func = ctx.source_value_as::<types::Function>(func);
        let func = func.await?;

        let arr = ctx.source_value_as::<types::Array>(arr);
        let arr = arr.await?;

        let task = ctx.task.clone();
        let func = func.map(|v| types::Portable::portable(v, ctx));
        make_value!([*func,*arr] {
            let (func_source, func) = func.take();
            let (arr_source, arr) = arr.take();
            let (func,arr) = task.join(func,arr).await?;
            let arr = task.join_all(arr.owned().0
                .into_iter()
                .map(|d| func.call(FunctionArguments::positional(vec![arr_source.clone().with(d)]), func_source.clone())));

            Ok(types::Array(arr.await?.into_iter().map(|v| v.unwrap()).collect()))
        }).into()
    }).into()
}

fn entries_fn() -> Value {
    ergo_function!(std::collection::entries, |ctx| {
        let value = ctx.args.next().ok_or("map not provided")?;

        ctx.unused_arguments()?;

        let map = ctx.source_value_as::<types::Map>(value);
        let map = map.await?.unwrap();

        make_value!([map] {
            let mut vals = RVec::new();
            for (k,v) in map.await?.owned().0 {
                vals.push(crate::grease_string_map! {
                    "Key-value pair from an entry in a map."
                    "key": "The key in the map." = k,
                    "value": "The value in the map." = v
                });
            }

            Ok(types::Array(vals))
        })
        .into()
    })
    .into()
}

fn has_fn() -> Value {
    ergo_function!(std::collection::has, |ctx| {
        let value = ctx.args.next().ok_or("value not provided")?.unwrap();
        let index = ctx.args.next().ok_or("index not provided")?;

        ctx.unused_arguments()?;

        use ergo_runtime::context_ext::AsContext;
        let ctx: grease::runtime::Context = ctx.as_context().clone();

        make_value!([value, *index] {
            match_value!(value => {
                types::Array => |val| {
                    let index = ctx.source_value_as::<types::String>(index);
                    let index = index.await?.unwrap().await?;
                    use std::str::FromStr;
                    let i = usize::from_str(index.as_ref())?;
                    i < val.await?.0.len()
                }
                types::Map => |val| {
                    val.await?.0.get(&index).is_some()
                }
                => |_| Err("non-indexable value")?
            }).await
        })
        .into()
    })
    .into()
}

fn get_fn() -> Value {
    ergo_function!(std::collection::get, |ctx| {
        let value = ctx.args.next().ok_or("value not provided")?.unwrap();
        let index = ctx.args.next().ok_or("index not provided")?;

        ctx.unused_arguments()?;

        use ergo_runtime::context_ext::AsContext;
        let ctx: grease::runtime::Context = ctx.as_context().clone();

        let deps = depends![value, *index];

        Value::dyn_new(
            async move {
                match_value!(value => {
                    types::Array => |val| {
                        let index = ctx.source_value_as::<types::String>(index);
                        let index = index.await?.unwrap().await?;
                        use std::str::FromStr;
                        let i = usize::from_str(index.as_ref())?;
                        val.await?.0.get(i).cloned().unwrap_or(types::Unit.into()).into_any_value()
                    }
                    types::Map => |val| {
                        val.await?.0.get(&index).cloned().unwrap_or(types::Unit.into()).into_any_value()
                    }
                    => |_| Err("non-indexable value")?
                })
                .await
            },
            deps,
        )
    })
    .into()
}

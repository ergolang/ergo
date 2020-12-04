//! Script runtime module.

use ergo_runtime::{ergo_function, types, ContextExt, ResultIterator};
use grease::{
    path::PathBuf,
    value::{IntoValue, Value},
};

pub fn module() -> Value {
    crate::grease_string_map! {
        "A map of script-related functions:"
        "bindings": "Get all of the current bindings in the script at the call site." = bindings_fn(),
        "set-load-path": "Set the load path for any `ergo` calls." = set_load_path_fn()
    }
}

fn bindings_fn() -> Value {
    ergo_function!(independent std::script::bindings, |ctx| {
        ctx.unused_arguments()?;

        let env = ctx.env_flatten();

        types::Map(env.into_iter()
            .map(|(k,v)| Ok((k,v.into_result()?.unwrap())))
            .collect_result()?
        ).into_value(ctx)
    })
    .into()
}

fn set_load_path_fn() -> Value {
    ergo_function!(independent std::script::set_load_path, |ctx| {
        let path = ctx.args.next().ok_or("no load path provided")?;

        ctx.unused_arguments()?;

        let path = ctx.source_value_as::<types::Array>(path).await?;

        let source = path.source();
        let ps: Vec<_> = path.unwrap().await?.owned().0.into_iter().map(|v| source.clone().with(v)).collect();

        let mut paths: abi_stable::std_types::RVec<PathBuf> = Default::default();
        for p in ps {
            paths.push(ctx.source_value_as::<PathBuf>(p).await?.unwrap().await?.owned());
        }

        ctx.load_paths = paths;

        types::Unit.into_value().into()
    })
    .into()
}

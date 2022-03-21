//! Base environment.

use ergo_runtime::{traits, types, Value};

mod doc;
mod load;

pub use doc::*;
pub use load::*;

#[types::ergo_fn]
/// The 'fn' binding function, which takes all arguments and returns an Args to be bound.
pub async fn fn_(...) -> Value {
    types::Args { args: REST }.into()
}

#[types::ergo_fn]
/// The 'index' function, which supports binding to Index values.
pub async fn index(binding: _) -> Value {
    types::Index(binding).into()
}

#[types::ergo_fn]
/// Bind the first argument using the value of the second argument.
pub async fn bind(to: _, from: _) -> Value {
    traits::bind(to, from).await
}

/// An `Unset` value.
pub fn unset() -> Value {
    types::Unset.into()
}

#[types::ergo_fn]
#[forced]
/// Mark the given value as pertinent to the identity of the result.
///
/// Arguments: `:value`
///
/// This means that the given value will be evaluated when the identity is needed.
pub async fn force(mut v: _) -> Value {
    drop(ergo_runtime::Context::eval(&mut v).await);
    v
}

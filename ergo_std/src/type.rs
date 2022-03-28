//! Type-related functions, including type creation.

use ergo_runtime::abi_stable::{
    std_types::{RArc, RString},
    type_erase::Erased,
};
use ergo_runtime::{
    depends, nsid, traits, try_result,
    type_system::{ergo_trait_impl, ErgoType, Type},
    types, Context, Value,
};
use futures::FutureExt;

pub fn r#type() -> Value {
    types::Type {
        tp: types::Type::ergo_type(),
        index: crate::make_string_map! {
            "any" = any(),
            "get" = get(),
            "index" = index(),
            "modify" = modify(),
            "new" = new()
        },
    }
    .into()
}

#[types::ergo_fn]
/// Create a new Type.
///
/// Arguments: `(String :id)`
///
/// `id` is used to derive the type identity, and must be unique to this type.
///
/// Returns a Type which matches values of the new Type (by application). It will also support the
/// following indices:
/// * `new` - a Function which creates a new instance of the Type (storing any arguments), and
/// * `@` - a Function which matches an instance of the Type and the stored arguments.
///
/// ## Example Usage
/// Create type:
/// ```ergo
/// MyType = Type:new "library:MyType"
/// ```
///
/// Create (compose) a value:
/// ```
/// val = MyType:new 1 2 3
/// ```
///
/// Match a value (checking type):
/// ```
/// MyType :v = $val
/// ```
///
/// Decompose a value (binding inner values):
/// ```
/// MyType:@ :a :b :c = $val
/// ```
///
/// Decompose a value (matching inner vaules):
/// ```
/// MyType:@ 1 2 3 = $val
/// ```
async fn new(id: types::String) -> Value {
    let tp = Type::named(id.as_ref().as_str().as_bytes());
    let id = id.to_owned().0;

    Context::global()
        .traits
        .add_impl::<traits::TypeName>(tp.clone(), {
            let mut imp = ergo_trait_impl! {
                impl traits::TypeName for _ {
                    fn type_name() -> RString {
                        unsafe { TRAIT_DATA.as_ref::<RString>() }.clone()
                    }
                }
            };
            imp.ergo_trait_data = Erased::new::<RString>(id.clone());
            imp
        });

    let new: Value = {
        let tp = tp.clone();
        let deps = depends![const tp, nsid!(new)];
        types::Unbound::new(
            move |arg| {
                let tp = tp.clone();
                async move {
                    let deps = depends![dyn tp, arg];
                    unsafe { Value::new(RArc::new(tp), RArc::new(Erased::new(arg)), deps) }
                }
                .boxed()
            },
            deps,
            format!(
                "Create a new instance of {}.\n\nAny arguments are stored in the instance.",
                &id
            ),
        )
        .into()
    };

    let bind: Value = {
        let tp = tp.clone();
        let deps = depends![const tp, nsid!(bind)];
        types::Unbound::new(move |arg| {
                let tp = tp.clone();
                async move {
                    let to_bind = arg;
                    let deps = depends![dyn tp, nsid!(bind), to_bind];
                    types::Unbound::new_no_doc(move |mut arg| {
                        let to_bind = to_bind.clone();
                        let tp = tp.clone();
                        async move {
                            try_result!(Context::eval(&mut arg).await);
                            if arg.ergo_type().unwrap() != &tp {
                                return traits::type_error_for_t(arg, &tp).into_error().into();
                            }
                            let data = arg.data().unwrap();
                            let data = unsafe { (*data).as_ref::<Value>() };
                            traits::bind(to_bind, data.clone()).await
                        }.boxed()
                    }, deps).into()
                }.boxed()
            },
            deps,
            format!("Bind to an instance of {}.\n\nAny arguments stored in the instance are bound to the arguments of this function.", &id),
        ).into()
    };

    types::Type {
        tp,
        index: crate::make_string_map! {
            "new" = new,
            "@" = bind
        },
    }
    .into()
}

#[types::ergo_fn]
/// Get the index Map of a Type.
///
/// Arguments: `(Type :type)`
///
/// Returns the Map used for indexing `type`.
async fn index(r#type: types::Type) -> Value {
    r#type.as_ref().index.clone().into()
}

#[types::ergo_fn]
/// Modify the index Map of a Type.
///
/// Arguments: `(Type :type) (Map :indices)`
///
/// Returns `type` alterend with the given `indices` map.
async fn modify(r#type: types::Type, indices: types::Map) -> Value {
    let mut ret = r#type.as_ref().clone();
    ret.index = indices.into();
    ret.into()
}

#[types::ergo_fn]
/// Get the Type of a value.
///
/// Arguments: `:value`
///
/// Keyed Arguments:
/// * `no-eval` - If present, don't evaluate `value`, returning `Unset` if `value` doesn't have a type.
/// * `allow-error` - If present, return the `Error` type rather than propagating an error.
///
/// Evaluates `value` to get the Type.
async fn get(mut value: _, (no_eval): [_], (allow_error): [_]) -> Value {
    if no_eval.is_none() {
        let r = Context::eval(&mut value).await;
        if allow_error.is_none() {
            r?;
        }
    }

    match value.ergo_type() {
        Some(t) => types::Type {
            tp: t.clone(),
            index: types::Map(Default::default()).into(),
        }
        .into(),
        None => types::Unset.into(),
    }
}

#[types::ergo_fn]
/// Evaluate a value to be typed when bound.
///
/// Arguments: `:target`
///
/// Keyed Arguments:
/// * `allow-error` - If present, allow errors to be evaluated without propagating.
async fn any(target: _, (allow_error): [_]) -> Value {
    let allow_error = allow_error.is_some();
    let deps = depends![dyn nsid!(std::Type::any), allow_error, target];

    types::Unbound::new_no_doc(
        move |mut arg| {
            let target = target.clone();
            async move {
                let r = Context::eval(&mut arg).await;
                if !allow_error {
                    try_result!(r);
                }
                traits::bind(target, arg).await
            }
            .boxed()
        },
        deps,
    )
    .into()
}

#[cfg(test)]
mod test {
    ergo_script::tests! {
        fn new(t) {
            t.assert_eq("MyType = self:Type:new \"scoped:my_type\"
            val = MyType:new str ()
            fn (MyType _) -> () |> $val
            MyType:@ :x :y = $val
            [$x,$y]",
                "[str,()]"
            );
        }

        fn index(t) {
            t.assert_eq("MyType = self:Type:new \"scoped:my_type\"
            { :new, :@ } = self:Type:index $MyType
            @ :v = new str
            $v",
                "str"
            );
        }

        fn modify(t) {
            t.assert_eq("MyType = self:Type:new \"scoped:my_type\"
            MyType = self:Type:modify $MyType { hello = fn :x -> MyType:new \"Hello, $x\", @ = MyType:@ }
            MyType:@ :n = MyType:hello world
            $n",
                "\"Hello, world\""
            );
        }

        fn get(t) {
            t.assert_eq("self:Type:get str", "self:String");
            t.assert_eq("self:Type:get [a,b]", "self:Array");
            t.assert_eq("self:Type:get {a=1}", "self:Map");
            t.assert_eq("self:Type:get ()", "self:Unit");
            t.assert_eq("self:Type:get $unset", "self:Unset");
            t.assert_fail("self:Type:get <| self:Error:new err");
            t.assert_eq("self:Type:get ^allow-error <| self:Error:new err", "self:Error");
            t.assert_eq("self:Type:get ^no-eval {[a,b]}", "$unset");
            t.assert_eq("self:Type :MyType = self:Type:new my_type
            i = MyType:new a b c
            self:match (self:Type:get $i) [$MyType -> ()]
            m = { :$MyType = () }
            self:Type :t = self:Type:get $i
            m:$t",
                "()"
            );
        }

        fn any(t) {
            t.assert_success("self:Type:any _ = hello");
            t.assert_fail("self:Type:any _ = self:Error:new err");
            t.assert_success("self:Type:any ^allow-error _ = self:Error:new err");
        }
    }
}

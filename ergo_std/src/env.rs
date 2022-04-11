//! Environment variable functions.

use ergo_runtime::{metadata::Doc, traits, types, value::match_value, Context, Value};

pub fn module() -> Value {
    crate::make_string_map! {
        "home" = home(),
        "path-search" = path_search(),
        "current-dir" = current_dir(),
        "user-cache" = user_cache(),
        "system-cache" = system_cache(),
        "config" = config(),
        "os" = os(),
        "arch" = arch(),
        "vars" = vars(),
        "concurrent-tasks" = concurrent_tasks()
    }
}

fn vars() -> Value {
    let mut m = types::Map(Default::default());
    for (k, v) in std::env::vars_os() {
        m.0.insert(
            types::String::from(k.to_string_lossy()).into(),
            types::String::from(v.to_string_lossy()).into(),
        );
    }
    let mut v = m.into();
    Doc::set_string(
        &mut v,
        "A Map of all environment variables when the process started, where keys and values are Strings."
    );
    v
}

fn current_dir() -> Value {
    let mut v = match std::env::current_dir().ok() {
        Some(path) => types::Path::from(path).into(),
        None => ergo_runtime::error! {
            error: "the current working directory couldn't be retrieved"
        }
        .into(),
    };
    Doc::set_string(
        &mut v,
        "The current working directory of the process, as a path.",
    );
    v
}

fn user_cache() -> Value {
    let path = directories::ProjectDirs::from("", "", "ergo").map(|d| d.cache_dir().to_owned());
    let mut v = match path {
        Some(path) => types::Path::from(path).into(),
        None => ergo_runtime::error! {
            error: "the user cache directory could not be retrieved"
        }
        .into(),
    };
    Doc::set_string(&mut v, "A user-level cache directory path.");
    v
}

fn system_cache() -> Value {
    let mut path = if cfg!(unix) {
        Some(std::path::Path::new("/var/cache/ergo"))
    } else if cfg!(windows) {
        Some(std::path::Path::new("C:\\Program Files\\ergo\\cache"))
    } else {
        None
    };
    if let Some(p) = &path {
        match p.metadata() {
            Err(_) => path = None,
            Ok(m) => {
                if !m.is_dir() || m.permissions().readonly() {
                    path = None;
                }
            }
        }
    }

    let path = path.map(|d| d.to_owned()).or_else(|| {
        directories::ProjectDirs::from("", "", "ergo").map(|d| d.cache_dir().to_owned())
    });

    let mut v = match path {
        Some(path) => types::Path::from(path).into(),
        None => ergo_runtime::error! {
            error: "the system cache directory could not be retrieved"
        }
        .into(),
    };
    Doc::set_string(&mut v, "A system-level cache directory path.");
    v
}

fn config() -> Value {
    let path =
        directories::ProjectDirs::from("", "", "ergo").map(|d| d.preference_dir().to_owned());
    let mut v = match path {
        Some(path) => types::Path::from(path).into(),
        None => ergo_runtime::error! {
            error: "the user configuration directory could not be retrieved"
        }
        .into(),
    };
    Doc::set_string(&mut v, "A user-level configuration directory path.");
    v
}

fn home() -> Value {
    let path = directories::BaseDirs::new().map(|d| d.home_dir().to_owned());
    let mut v = match path {
        Some(path) => types::Path::from(path).into(),
        None => ergo_runtime::error! {
            error: "the home path could not be retrieved"
        }
        .into(),
    };
    Doc::set_string(&mut v, "The current user's home directory path.");
    v.into()
}

#[types::ergo_fn]
/// Find a file in the binary lookup path.
///
/// Arguments: `(StringOrPath :name)`
///
/// If a Path is passed, it will simply be returned as-is. If a String is passed, it will search
/// for the string in PATH. If not found, Unset is returned. Otherwise the Path of the resolved
/// file is returned.
async fn path_search(mut string_or_path: _) -> Value {
    Context::eval(&mut string_or_path).await?;
    match_value! { string_or_path,
        p@types::Path(_) => p.into(),
        types::String(name) => {
            let paths = std::env::var_os("PATH")
                .map(|path| std::env::split_paths(&path).collect())
                .unwrap_or(vec![]);

            for p in paths {
                let path = p.join(name.as_str());
                if path.is_file() {
                    return Ok(types::Path::from(path).into());
                }
            }

            types::Unset.into()
        }
        other => Err(traits::type_error(other, "String or Path"))?
    }
}

fn os() -> Value {
    let mut v: Value = types::String::from(std::env::consts::OS).into();
    Doc::set_string(
        &mut v,
        "The OS running ergo.

Possible values include `linux`, `macos`, and `windows`.",
    );
    v
}

fn arch() -> Value {
    let mut v: Value = types::String::from(std::env::consts::ARCH).into();
    Doc::set_string(
        &mut v,
        "The architecture for which ergo was built.

Possible values include `x86_64`, `x86`, `arm`, and `aarch64`.",
    );
    v
}

fn concurrent_tasks() -> Value {
    let mut v: Value = types::Number::from(Context::global().task.threads()).into();
    Doc::set_string(
        &mut v,
        "The maximum number of concurrent tasks that can run at once.",
    );
    v
}

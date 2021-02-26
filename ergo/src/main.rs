use abi_stable::std_types::{RDuration, ROption, RSlice, RString};
use ergo_runtime::{source::Source, ContextExt};
use ergo_script::constants::{PROGRAM_NAME, WORKSPACE_NAME};
use grease::runtime::*;
use simplelog::WriteLogger;
use std::io::Write;
use std::str::FromStr;

mod options;
mod output;
mod script;
mod sync;

/// Constant values shared throughout the program.
mod constants {
    use directories;
    pub fn app_dirs() -> Option<directories::ProjectDirs> {
        directories::ProjectDirs::from("", "", super::PROGRAM_NAME)
    }
}

use options::*;
use output::{error as error_output, output, Output};
use script::{Script, StringSource};

trait AppErr {
    type Output;

    fn app_err_result(self, s: &str) -> Result<Self::Output, String>;

    fn app_err(self, s: &str) -> Self::Output
    where
        Self: Sized,
    {
        match self.app_err_result(s) {
            Err(e) => err_exit(&e),
            Ok(v) => v,
        }
    }
}

fn err_exit(s: &str) -> ! {
    writeln!(std::io::stderr(), "{}", s).unwrap();
    std::io::stderr().flush().unwrap();
    std::process::exit(1);
}

impl<T, E: std::fmt::Display> AppErr for Result<T, E> {
    type Output = T;

    fn app_err_result(self, s: &str) -> Result<Self::Output, String> {
        self.map_err(move |e| format!("{}:\n{}", s, e))
    }
}

impl<T> AppErr for Option<T> {
    type Output = T;

    fn app_err_result(self, s: &str) -> Result<Self::Output, String> {
        self.ok_or(s.to_owned())
    }
}

impl AppErr for bool {
    type Output = ();

    fn app_err_result(self, s: &str) -> Result<Self::Output, String> {
        if !self {
            Err(s.to_owned())
        } else {
            Ok(())
        }
    }
}

fn run(opts: Opts) -> Result<String, String> {
    let mut output = output(opts.format, !opts.stop)
        .app_err_result("could not create output from requested format")?;
    output.set_log_level(opts.log_level);
    let logger = std::sync::Arc::new(std::sync::Mutex::new(output));

    #[derive(Clone)]
    struct WeakLogTarget<T>(std::sync::Weak<std::sync::Mutex<T>>);

    impl<T: LogTarget + Send> WeakLogTarget<T> {
        fn with<R, F: FnOnce(&mut (dyn LogTarget + Send)) -> R>(&self, f: F) -> Option<R> {
            if let Some(logger) = self.0.upgrade() {
                if let Ok(mut logger) = logger.lock() {
                    return Some(f(&mut *logger));
                }
            }
            None
        }
    }

    impl<T: LogTarget + Send> LogTarget for WeakLogTarget<T> {
        fn log(&mut self, entry: LogEntry) {
            self.with(move |l| l.log(entry));
        }

        fn task_running(&mut self, description: RString) -> LogTaskKey {
            self.with(move |l| l.task_running(description))
                .unwrap_or(LogTaskKey::new(()))
        }

        fn task_suspend(&mut self, key: LogTaskKey) {
            self.with(move |l| l.task_suspend(key));
        }

        fn timer_pending(&mut self, id: RSlice<RString>) {
            self.with(move |l| l.timer_pending(id));
        }

        fn timer_complete(&mut self, id: RSlice<RString>, duration: ROption<RDuration>) {
            self.with(move |l| l.timer_complete(id, duration));
        }

        fn pause_logging(&mut self) {
            self.with(move |l| l.pause_logging());
        }

        fn resume_logging(&mut self) {
            self.with(move |l| l.resume_logging());
        }
    }

    let working_dir = std::env::current_dir().expect("could not get current directory");

    // Search for furthest workspace ancestor, and set as storage directory root
    let storage_dir_root = if let Some(p) = working_dir
        .ancestors()
        .filter(|p| p.join(WORKSPACE_NAME).exists())
        .last()
    {
        p.to_owned()
    } else {
        working_dir.clone()
    };

    let storage_directory = storage_dir_root.join(opts.storage);
    if opts.clean && storage_directory.exists() {
        std::fs::remove_dir_all(&storage_directory)
            .app_err_result("failed to clean storage directory")?;
    }

    // Get initial load path from exe location and user directories.
    let initial_load_path = {
        let mut load_paths = Vec::new();

        // Add neighboring share directories when running in a [prefix]/bin directory.
        let mut neighbor_dir = std::env::current_exe().ok().and_then(|path| {
            path.parent().and_then(|parent| {
                if parent.file_name() == Some("bin".as_ref()) {
                    let path = parent
                        .parent()
                        .expect("must have parent directory")
                        .join("share")
                        .join(PROGRAM_NAME)
                        .join("lib");
                    if path.exists() {
                        Some(path)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
        });

        // If the neighbor directory is somewhere in the home directory, it should be added prior to the local
        // data app dir.
        if let (Some(dir), Some(user_dirs)) = (&neighbor_dir, &directories::UserDirs::new()) {
            if dir.starts_with(user_dirs.home_dir()) {
                load_paths.push(neighbor_dir.take().unwrap());
            }
        }

        // Add local data app dir.
        if let Some(proj_dirs) = constants::app_dirs() {
            let path = proj_dirs.data_local_dir().join("lib");
            if path.exists() {
                load_paths.push(path);
            }
        }

        // If the neighbor directory wasn't added, it should be added now, after the local data app dir.
        if let Some(dir) = neighbor_dir {
            load_paths.push(dir);
        }

        load_paths
    };

    // Use a weak pointer to the logger in the context, so that when the logger goes out of
    // scope the output is cleaned up reliably. The runtime doesn't reliably shutdown as it
    // drops the ThreadPool asynchronously, and likewise for general logging we shouldn't rely
    // on values cleaning up after themselves.
    let weak_logger = std::sync::Arc::downgrade(&logger);

    let doc_write = opts.doc_path.is_some();

    // Create runtime context
    let mut ctx = script::script_context(
        Context::builder()
            .logger(WeakLogTarget(weak_logger.clone()))
            .storage_directory(storage_directory)
            .threads(opts.jobs)
            .keep_going(!opts.stop),
        initial_load_path,
        opts.doc_path,
    )
    .expect("failed to create script context");

    ctx.lint = opts.lint;

    // Set interrupt signal handler to abort tasks.
    //
    // Keep _task in scope until the end of the function. After the function exits,
    // the ctrlc handler will not hold onto the context task manager, which is important for
    // cleanup.
    let (_task, task_ref) = sync::Scoped::new_pair(ctx.task.clone());
    {
        ctrlc::set_handler(move || task_ref.with(|t| t.abort()))
            .app_err_result("failed to set signal handler")?;
    }

    let mut eval_args = String::new();
    let no_args = opts.args.is_empty();
    for arg in opts.args {
        if !eval_args.is_empty() {
            eval_args.push(' ');
        }
        eval_args.push_str(&arg);
    }

    // Ensure load path is set for path resolution done below.
    ctx.reset_load_path();

    let mut to_eval = if opts.expression {
        if no_args {
            "()".into()
        } else {
            eval_args
        }
    } else {
        // When there are no arguments, always call workspace:command function.
        // This returns _just_ the function, so that documentation will work as expected, but it
        // will then be called without arguments (with script::final_value).
        if no_args {
            "workspace:command".into()
        } else {
            // When there are arguments:
            // * Replace the first `:` with `|>:` for convenience.
            // * Inspect the first effective argument to determine whether to invoke a normal load
            // or the workspace:command function.
            let first_arg_end = if let Some(p) = eval_args.find(':') {
                if eval_args.get(p - 2..p).map(|s| s != "|>").unwrap_or(true) {
                    eval_args.replace_range(p..p + 1, "|>:");
                }
                p
            } else if let Some(p) = eval_args.find(&[' ', '|', '<', '{', '[', '('][..]) {
                p
            } else {
                eval_args.len()
            };
            let first_arg = &eval_args[0..first_arg_end];
            let function = if ergo_script::resolve_script_path(&ctx, first_arg.as_ref()).is_none() {
                "workspace:command"
            } else {
                PROGRAM_NAME
            };
            format!("{} {}", function, eval_args)
        }
    };

    if opts.doc {
        to_eval = format!("doc ({})", to_eval);
    } else if doc_write {
        to_eval = format!("doc:write . ({})", to_eval);
    }

    let source = Source::new(StringSource::new("<command line>", to_eval));
    let loaded = Script::load(source).app_err_result("failed to parse script file")?;

    // Setup error observer to display in logging.
    let on_error = move |e: grease::Error| {
        if let Some(logger) = weak_logger.upgrade() {
            if let Ok(mut logger) = logger.lock() {
                logger.new_error(e);
            }
        }
    };

    let task = ctx.task.clone();
    let script_output = task.block_on(grease::value::Errored::observe(
        on_error.clone(),
        loaded.evaluate(&ctx),
    ));

    // We *must* keep the context around because it holds onto plugins, which may have functions referenced in values.
    // Likewise, our return from this function is "flattened" to strings to ensure any references to plugins are used when the plugins are still loaded.

    let value_to_execute = script_output.and_then(|script_output| {
        task.block_on(grease::value::Errored::observe(
            on_error.clone(),
            script::final_value(&mut ctx, script_output),
        ))
        .map(Source::unwrap)
    });

    // Clear context, which removes any unneeded values that may be held.
    //
    // This is *important* as some values (like those returned by exec) behave differently if their
    // peers still exist or not.
    ctx.clear_env();

    let ret = value_to_execute.and_then(|value| {
        task.block_on(grease::value::Errored::observe(on_error, async {
            ctx.force_value_nested(value.clone()).await?;
            let mut s = String::new();
            {
                let mut formatter = ergo_runtime::traits::Formatter::new(&mut s);
                ctx.display(value, &mut formatter).await?;
            }
            Ok(s)
        }))
    });

    // Before the context is destroyed (unloading plugins), clear the thread-local storage in case
    // there are values which were allocated in the plugins.
    ergo_runtime::plugin::Context::reset();

    // Drop the logger prior to the context dropping, so that any stored state (like errors) can
    // free with the plugins still loaded.
    // Only drop when nothing else is using it (so we have reliable terminal cleanup).
    let mut logger = Some(logger);
    loop {
        match std::sync::Arc::try_unwrap(logger.take().unwrap()) {
            Ok(l) => {
                drop(l);
                break;
            }
            Err(l) => logger = Some(l),
        }
    }

    // Write error output to stderr.
    match ret {
        Ok(v) => Ok(v),
        Err(e) => {
            use grease::error::BoxGreaseError;

            // For each error root, display a certain number of backtrace contexts.
            fn display_errors<'a>(
                e: &'a BoxGreaseError,
                out: &mut Box<term::StderrTerminal>,
                contexts: &mut Vec<&'a grease::error::Context>,
                limit: &Option<usize>,
            ) {
                let had_context = if let Some(ctx) = e.downcast_ref::<grease::error::ErrorContext>()
                {
                    contexts.push(ctx.context());
                    true
                } else {
                    false
                };
                let inner = e.source();
                if inner.is_empty() {
                    // Root error
                    out.fg(term::color::RED)
                        .expect("failed to set output color");
                    write!(out, "error:").expect("failed to write to stderr");
                    out.reset().expect("failed to reset output");
                    writeln!(out, " {}", e).expect("failed to write to stderr");
                    let write_context = |ctx: &&grease::error::Context| {
                        out.fg(term::color::BRIGHT_WHITE)
                            .expect("failed to set output color");
                        write!(out, "note:").expect("failed to write to stderr");
                        out.reset().expect("failed to reset output");
                        writeln!(out, " {}", ctx).expect("failed to write to stderr");
                    };
                    match limit.as_ref() {
                        None => contexts.iter().rev().for_each(write_context),
                        Some(&limit) => {
                            contexts.iter().rev().take(limit).for_each(write_context);
                            if limit < contexts.len() {
                                writeln!(out, "...omitting {} frame(s)", contexts.len() - limit)
                                    .expect("failed to write to stderr");
                            }
                        }
                    }
                } else {
                    for e in inner {
                        display_errors(e, out, contexts, limit);
                    }
                }
                if had_context {
                    contexts.pop();
                }
            }

            let mut err = error_output(opts.format)
                .app_err("could not create error output from requested format");

            let mut contexts = Vec::new();
            display_errors(&e.error(), &mut err, &mut contexts, &opts.error_limit);
            Err("one or more errors occurred".into())
        }
    }
}

fn main() {
    // Install the application logger.
    //
    // We write all logs to the configured (by env variable) file, falling back to the data local
    // dir or temp directory, and read from the {PROGRAM_NAME}_LOG environment variable to set the
    // application log level (which defaults to warn).
    {
        let log_file = {
            if let Some(f) = std::env::var_os(format!("{}_LOG_FILE", PROGRAM_NAME.to_uppercase())) {
                f.into()
            } else {
                let mut f = if let Some(proj_dirs) = constants::app_dirs() {
                    proj_dirs.data_local_dir().to_owned()
                } else {
                    std::env::temp_dir()
                };
                f.push(format!("{}.log", PROGRAM_NAME));
                f
            }
        };
        std::fs::create_dir_all(log_file.parent().expect("log file is a root directory"))
            .expect("failed to create log output directory");
        WriteLogger::init(
            std::env::var(format!("{}_LOG", PROGRAM_NAME.to_uppercase()))
                .map(|v| simplelog::LevelFilter::from_str(&v).expect("invalid program log level"))
                .unwrap_or(simplelog::LevelFilter::Warn),
            simplelog::Config::default(),
            std::fs::File::create(log_file).expect("failed to open log file for writing"),
        )
        .unwrap();
    }

    // Parse arguments (we handle help flags ourselves)
    let mut app = Opts::clap().settings(&[
        structopt::clap::AppSettings::DisableHelpFlags,
        structopt::clap::AppSettings::TrailingVarArg,
    ]);
    let mut opts = Opts::from_clap(&app.clone().get_matches());

    // Additional opts logic.
    if opts.doc {
        opts.page ^= true;
    }

    // Handle help flags
    // We explicitly handle them to add the documentation of the ergo function to `--help`.
    if opts.help {
        let long_help = std::env::args_os().any(|a| a == "--help");
        if !long_help {
            app.print_help().app_err("writing help text failed");
        } else {
            app.print_long_help().app_err("writing help text failed");
        }
        println!();

        if long_help {
            // Document load command.
            println!(
                "\n`ergo` command documentation:\n{}",
                script::LOAD_DOCUMENTATION
            );
        }

        std::process::exit(0);
    }

    let paging_enabled = opts.page;

    let result = run(opts);

    if paging_enabled {
        pager::Pager::with_default_pager(if cfg!(target_os = "macos") {
            // macos less is old and buggy, not working correctly with `-F`
            "less"
        } else {
            "less -F"
        })
        .setup();
    }

    match result {
        Ok(s) => writeln!(std::io::stdout(), "{}", s).expect("writing output failed"),
        Err(e) => err_exit(&e),
    }
}

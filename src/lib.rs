//! A small Rust facade over uv's in-process command dispatcher.
//!
//! This crate pins uv as a Git submodule. uv's `run` entry point is internal and
//! changes across releases, so update uv and this adapter together.

use std::error::Error;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;

use clap::Parser;
use tokio::sync::Mutex;
use uv::GlobalInitialization;
use uv_cli::Cli;
use uv_command_support::ExitStatus;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

static UV_STATE: OnceLock<Mutex<UvState>> = OnceLock::new();

enum UvState {
    Uninitialized,
    Ready,
    Poisoned,
}

/// A virtual environment managed by uv and used by a Rust host application.
#[derive(Clone, Debug)]
pub struct PythonEnvironment {
    path: PathBuf,
}

impl PythonEnvironment {
    /// Construct a handle to an environment at an absolute path.
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let path = std::path::absolute(path)?;
        Ok(Self { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Path to the environment's Python executable.
    pub fn python(&self) -> PathBuf {
        #[cfg(windows)]
        {
            self.path.join("Scripts").join("python.exe")
        }
        #[cfg(not(windows))]
        {
            self.path.join("bin").join("python")
        }
    }

    /// Construct a Python command for custom arguments, streaming, or timeouts.
    pub fn python_command(&self) -> Command {
        Command::new(self.python())
    }

    /// Create this environment with uv. `python_request` may be a version such as `3.12`.
    pub async fn create(&self, python_request: Option<&str>) -> Result<()> {
        let mut args = vec![OsString::from("venv"), self.path.as_os_str().to_owned()];
        if let Some(request) = python_request {
            args.push(OsString::from("--python"));
            args.push(OsString::from(request));
        }
        invoke(args).await
    }

    /// Install PEP 508 requirements into this environment.
    pub async fn install(&self, requirements: &[&str]) -> Result<()> {
        if requirements.is_empty() {
            return Ok(());
        }
        let mut args = self.pip_args("install");
        args.extend(
            requirements
                .iter()
                .map(|requirement| OsString::from(*requirement)),
        );
        invoke(args).await
    }

    /// Remove packages from this environment.
    pub async fn uninstall(&self, packages: &[&str]) -> Result<()> {
        if packages.is_empty() {
            return Ok(());
        }
        let mut args = self.pip_args("uninstall");
        args.extend(packages.iter().map(|package| OsString::from(*package)));
        invoke(args).await
    }

    /// Synchronize the environment with a requirements file.
    pub async fn sync_requirements(&self, requirements_file: impl AsRef<Path>) -> Result<()> {
        let mut args = self.pip_args("sync");
        args.push(std::path::absolute(requirements_file)?.into_os_string());
        invoke(args).await
    }

    /// Run a Python script in a child process and capture its output.
    pub fn run_script(
        &self,
        script: impl AsRef<OsStr>,
        args: &[&OsStr],
    ) -> std::io::Result<Output> {
        self.python_command().arg(script).args(args).output()
    }

    /// Run Python source with `-c` in a child process and capture its output.
    pub fn run_code(&self, code: &str) -> std::io::Result<Output> {
        self.python_command().arg("-c").arg(code).output()
    }

    fn pip_args(&self, command: &str) -> Vec<OsString> {
        vec![
            OsString::from("pip"),
            OsString::from(command),
            OsString::from("--python"),
            self.python().into_os_string(),
        ]
    }
}

/// Invoke any uv subcommand in this process. Arguments omit the `uv` executable name.
///
/// uv writes directly to the host process's standard streams and maintains global
/// state. Calls through this adapter are serialized. Avoid uv options that change
/// the process working directory, such as `--directory`.
pub async fn invoke<I, S>(args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let mut argv = vec![OsString::from("uv")];
    argv.extend(args.into_iter().map(Into::into));
    let cli = Cli::try_parse_from(argv)?;

    let mut state = UV_STATE
        .get_or_init(|| Mutex::new(UvState::Uninitialized))
        .lock()
        .await;
    let (mode, first_call) = match *state {
        UvState::Uninitialized => (GlobalInitialization::Initialize, true),
        UvState::Ready => (GlobalInitialization::Reuse, false),
        UvState::Poisoned => {
            return Err("uv's first call failed; restart the host process before retrying".into());
        }
    };
    let result = Box::pin(uv::run(cli, mode)).await;
    if first_call {
        *state = if result.is_ok() {
            UvState::Ready
        } else {
            UvState::Poisoned
        };
    }
    let status = result?;
    match status {
        ExitStatus::Success => Ok(()),
        ExitStatus::Failure => Err("uv reported an input or resolution failure".into()),
        ExitStatus::Error => Err("uv reported an internal or environment error".into()),
        ExitStatus::External(code) => {
            Err(format!("uv child process exited with code {code}").into())
        }
    }
}

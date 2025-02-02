use super::{Command, Error, ForwardType, KnownHosts, SessionBuilder, Socket};

#[cfg(feature = "process-mux")]
use super::process_impl;

#[cfg(feature = "native-mux")]
use super::native_mux_impl;

use std::borrow::Cow;
use std::ffi::OsStr;
use std::path::Path;

#[derive(Debug)]
pub(crate) enum SessionImp {
    #[cfg(feature = "process-mux")]
    ProcessImpl(process_impl::Session),

    #[cfg(feature = "native-mux")]
    NativeMuxImpl(native_mux_impl::Session),
}

#[cfg(any(feature = "process-mux", feature = "native-mux"))]
macro_rules! delegate {
    ($impl:expr, $var:ident, $then:block) => {{
        match $impl {
            #[cfg(feature = "process-mux")]
            SessionImp::ProcessImpl($var) => $then,

            #[cfg(feature = "native-mux")]
            SessionImp::NativeMuxImpl($var) => $then,
        }
    }};
}

#[cfg(not(any(feature = "process-mux", feature = "native-mux")))]
macro_rules! delegate {
    ($impl:expr, $var:ident, $then:block) => {{
        unreachable!("Neither feature process-mux nor native-mux is enabled")
    }};
}

/// A single SSH session to a remote host.
///
/// You can use [`command`](Session::command) to start a new command on the connected machine.
///
/// When the `Session` is dropped, the connection to the remote host is severed, and any errors
/// silently ignored. To disconnect and be alerted to errors, use [`close`](Session::close).
#[derive(Debug)]
pub struct Session(SessionImp);

#[cfg(feature = "process-mux")]
impl From<process_impl::Session> for Session {
    fn from(imp: process_impl::Session) -> Self {
        Self(SessionImp::ProcessImpl(imp))
    }
}

#[cfg(feature = "native-mux")]
impl From<native_mux_impl::Session> for Session {
    fn from(imp: native_mux_impl::Session) -> Self {
        Self(SessionImp::NativeMuxImpl(imp))
    }
}

// TODO: UserKnownHostsFile for custom known host fingerprint.

impl Session {
    /// Connect to the host at the given `host` over SSH using process impl, which will
    /// spawn a new ssh process for each `Child` created.
    ///
    /// The format of `destination` is the same as the `destination` argument to `ssh`. It may be
    /// specified as either `[user@]hostname` or a URI of the form `ssh://[user@]hostname[:port]`.
    ///
    /// If connecting requires interactive authentication based on `STDIN` (such as reading a
    /// password), the connection will fail. Consider setting up keypair-based authentication
    /// instead.
    ///
    /// For more options, see [`SessionBuilder`].
    #[cfg(feature = "process-mux")]
    #[cfg_attr(docsrs, doc(cfg(feature = "process-mux")))]
    pub async fn connect<S: AsRef<str>>(destination: S, check: KnownHosts) -> Result<Self, Error> {
        let mut s = SessionBuilder::default();
        s.known_hosts_check(check);
        s.connect(destination.as_ref()).await
    }

    /// Connect to the host at the given `host` over SSH using native mux impl, which
    /// will create a new socket connection for each `Child` created.
    ///
    /// See the crate-level documentation for more details on the difference between native and process-based mux.
    ///
    /// The format of `destination` is the same as the `destination` argument to `ssh`. It may be
    /// specified as either `[user@]hostname` or a URI of the form `ssh://[user@]hostname[:port]`.
    ///
    /// If connecting requires interactive authentication based on `STDIN` (such as reading a
    /// password), the connection will fail. Consider setting up keypair-based authentication
    /// instead.
    ///
    /// For more options, see [`SessionBuilder`].
    #[cfg(feature = "native-mux")]
    #[cfg_attr(docsrs, doc(cfg(feature = "native-mux")))]
    pub async fn connect_mux<S: AsRef<str>>(
        destination: S,
        check: KnownHosts,
    ) -> Result<Self, Error> {
        let mut s = SessionBuilder::default();
        s.known_hosts_check(check);
        s.connect_mux(destination.as_ref()).await
    }

    /// Check the status of the underlying SSH connection.
    #[cfg(not(windows))]
    #[cfg_attr(docsrs, doc(cfg(not(windows))))]
    pub async fn check(&self) -> Result<(), Error> {
        delegate!(&self.0, imp, { imp.check().await })
    }

    /// Get the SSH connection's control socket path.
    #[cfg(not(windows))]
    #[cfg_attr(docsrs, doc(cfg(not(windows))))]
    pub fn control_socket(&self) -> &Path {
        delegate!(&self.0, imp, { imp.ctl() })
    }

    /// Constructs a new [`Command`] for launching the program at path `program` on the remote
    /// host.
    ///
    /// Before it is passed to the remote host, `program` is escaped so that special characters
    /// aren't evaluated by the remote shell. If you do not want this behavior, use
    /// [`raw_command`](Session::raw_command).
    ///
    /// The returned `Command` is a builder, with the following default configuration:
    ///
    /// * No arguments to the program
    /// * Empty stdin and dsicard stdout/stderr for `spawn` or `status`, but create output pipes for
    ///   `output`
    ///
    /// Builder methods are provided to change these defaults and otherwise configure the process.
    ///
    /// If `program` is not an absolute path, the `PATH` will be searched in an OS-defined way on
    /// the host.
    pub fn command<'a, S: Into<Cow<'a, str>>>(&self, program: S) -> Command<'_> {
        self.raw_command(&*shell_escape::unix::escape(program.into()))
    }

    /// Constructs a new [`Command`] for launching the program at path `program` on the remote
    /// host.
    ///
    /// Unlike [`command`](Session::command), this method does not shell-escape `program`, so it may be evaluated in
    /// unforeseen ways by the remote shell.
    ///
    /// The returned `Command` is a builder, with the following default configuration:
    ///
    /// * No arguments to the program
    /// * Empty stdin and dsicard stdout/stderr for `spawn` or `status`, but create output pipes for
    ///   `output`
    ///
    /// Builder methods are provided to change these defaults and otherwise configure the process.
    ///
    /// If `program` is not an absolute path, the `PATH` will be searched in an OS-defined way on
    /// the host.
    pub fn raw_command<S: AsRef<OsStr>>(&self, program: S) -> Command<'_> {
        Command::new(
            self,
            delegate!(&self.0, imp, { imp.raw_command(program).into() }),
        )
    }

    /// Constructs a new [`Command`] for launching subsystem `program` on the remote
    /// host.
    ///
    /// Unlike [`command`](Session::command), this method does not shell-escape `program`, so it may be evaluated in
    /// unforeseen ways by the remote shell.
    ///
    /// The returned `Command` is a builder, with the following default configuration:
    ///
    /// * No arguments to the program
    /// * Empty stdin and dsicard stdout/stderr for `spawn` or `status`, but create output pipes for
    ///   `output`
    ///
    /// Builder methods are provided to change these defaults and otherwise configure the process.
    ///
    /// ## Sftp subsystem
    ///
    /// To use the sftp subsystem, you'll want to use [`openssh-sftp-client`],
    /// then use the following code to construct a sftp instance:
    ///
    /// [`openssh-sftp-client`]: https://crates.io/crates/openssh-sftp-client
    ///
    /// ```rust,no_run
    /// # use std::error::Error;
    /// # #[cfg(feature = "native-mux")]
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn Error>> {
    ///
    /// use openssh::{Session, KnownHosts, Stdio};
    /// use openssh_sftp_client::highlevel::Sftp;
    ///
    /// let session = Session::connect_mux("me@ssh.example.com", KnownHosts::Strict).await?;
    ///
    /// let mut child = session
    ///     .subsystem("sftp")
    ///     .stdin(Stdio::piped())
    ///     .stdout(Stdio::piped())
    ///     .spawn()
    ///     .await?;
    ///
    /// Sftp::new(
    ///     child.stdin().take().unwrap(),
    ///     child.stdout().take().unwrap(),
    ///     Default::default(),
    /// )
    /// .await?
    /// .close()
    /// .await?;
    ///
    /// # Ok(()) }
    /// ```
    pub fn subsystem<S: AsRef<OsStr>>(&self, program: S) -> Command<'_> {
        Command::new(
            self,
            delegate!(&self.0, imp, { imp.subsystem(program).into() }),
        )
    }

    /// Constructs a new [`Command`] that runs the provided shell command on the remote host.
    ///
    /// The provided command is passed as a single, escaped argument to `sh -c`, and from that
    /// point forward the behavior is up to `sh`. Since this executes a shell command, keep in mind
    /// that you are subject to the shell's rules around argument parsing, such as whitespace
    /// splitting, variable expansion, and other funkyness. I _highly_ recommend you read
    /// [this article] if you observe strange things.
    ///
    /// While the returned `Command` is a builder, like for [`command`](Session::command), you should not add
    /// additional arguments to it, since the arguments are already passed within the shell
    /// command.
    ///
    /// # Non-standard Remote Shells
    ///
    /// It is worth noting that there are really _two_ shells at work here: the one that sshd
    /// launches for the session, and that launches are command; and the instance of `sh` that we
    /// launch _in_ that session. This method tries hard to ensure that the provided `command` is
    /// passed exactly as-is to `sh`, but this is complicated by the presence of the "outer" shell.
    /// That outer shell may itself perform argument splitting, variable expansion, and the like,
    /// which might produce unintuitive results. For example, the outer shell may try to expand a
    /// variable that is only defined in the inner shell, and simply produce an empty string in the
    /// variable's place by the time it gets to `sh`.
    ///
    /// To counter this, this method assumes that the remote shell (the one launched by `sshd`) is
    /// [POSIX compliant]. This is more or less equivalent to "supports `bash` syntax" if you don't
    /// look too closely. It uses [`shell-escape`] to escape `command` before sending it to the
    /// remote shell, with the expectation that the remote shell will only end up undoing that one
    /// "level" of escaping, thus producing the original `command` as an argument to `sh`. This
    /// works _most of the time_.
    ///
    /// With sufficiently complex or weird commands, the escaping of `shell-escape` may not fully
    /// match the "un-escaping" of the remote shell. This will manifest as escape characters
    /// appearing in the `sh` command that you did not intend to be there. If this happens, try
    /// changing the remote shell if you can, or fall back to [`command`](Session::command)
    /// and do the escaping manually instead.
    ///
    ///   [POSIX compliant]: https://pubs.opengroup.org/onlinepubs/9699919799/xrat/V4_xcu_chap02.html
    ///   [this article]: https://mywiki.wooledge.org/Arguments
    ///   [`shell-escape`]: https://crates.io/crates/shell-escape
    pub fn shell<S: AsRef<str>>(&self, command: S) -> Command<'_> {
        let mut cmd = self.command("sh");
        cmd.arg("-c").arg(command);
        cmd
    }

    /// Request to open a local/remote port forwarding.
    /// The `Socket` can be either a unix socket or a tcp socket.
    ///
    /// If `forward_type` == Local, then `listen_socket` on local machine will be
    /// forwarded to `connect_socket` on remote machine.
    ///
    /// Otherwise, `listen_socket` on the remote machine will be forwarded to `connect_socket`
    /// on the local machine.
    ///
    /// Currently, there is no way of stopping a port forwarding due to the fact that
    /// openssh multiplex server/master does not support this.
    pub async fn request_port_forward(
        &self,
        forward_type: ForwardType,
        listen_socket: Socket<'_>,
        connect_socket: Socket<'_>,
    ) -> Result<(), Error> {
        delegate!(&self.0, imp, {
            imp.request_port_forward(forward_type, listen_socket, connect_socket)
                .await
        })
    }

    /// Terminate the remote connection.
    pub async fn close(self) -> Result<(), Error> {
        delegate!(self.0, imp, { imp.close().await })
    }
}

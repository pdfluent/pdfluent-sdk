//! Password sourcing: `--password-file`, environment, or prompt.
//!
//! Passwords are NEVER argv values: argv is visible in `ps` to every user on
//! the machine. The three sources, in precedence order:
//!
//! 1. `--password-file <path>` — line 1 is the password (line 2 the owner
//!    password for `protect`), so scripts can use files with locked-down
//!    permissions instead of the process list or the environment;
//! 2. `PDFLUENT_PASSWORD` (and `PDFLUENT_OWNER_PASSWORD` for `protect`);
//! 3. an interactive prompt, read from stdin — no-echo on a Unix TTY, piped
//!    input otherwise (which is also how tests drive it).
//!
//! Only trailing newlines are stripped: a password may start, end, or consist
//! entirely of spaces, and trimming them would change what was meant. A line
//! that is empty after that strip is not a password: where one is required it
//! is refused with `PASSWORD_REQUIRED`, never used as an empty password.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::io::{self, BufRead, IsTerminal, Write};
use std::path::Path;

use crate::output::Ctx;

/// Read line `line` (1-based) of a password file, minus its newline.
pub(crate) fn read_password_file(path: &Path, line: usize) -> Result<String, (String, String)> {
    let text = std::fs::read_to_string(path).map_err(|e| match e.kind() {
        io::ErrorKind::NotFound => (
            "FILE_NOT_FOUND".to_string(),
            format!("no such password file: {}", path.display()),
        ),
        _ => (
            "UNREADABLE_INPUT".to_string(),
            format!("could not read password file {}: {e}", path.display()),
        ),
    })?;
    text.lines()
        .nth(line - 1)
        .map(str::to_string)
        .ok_or_else(|| {
            (
                "PASSWORD_REQUIRED".to_string(),
                format!("password file {} has no line {line}", path.display()),
            )
        })
}

/// Resolve a REQUIRED password: file line 1, then env, then prompt.
pub(crate) fn required_password(
    ctx: &Ctx,
    command: &str,
    file: Option<&Path>,
    env_var: &str,
    label: &str,
) -> Result<String, u8> {
    if let Some(path) = file {
        let pw = read_password_file(path, 1)
            .map_err(|(code, msg)| crate::output::error(command, ctx, &code, &msg))?;
        // An empty line 1 is not a password: `protect` with it would write a
        // document that opens without one and report success.
        if pw.is_empty() {
            return Err(crate::output::error(
                command,
                ctx,
                "PASSWORD_REQUIRED",
                &format!(
                    "password file {} line 1 is empty; refusing an empty password",
                    path.display()
                ),
            ));
        }
        return Ok(pw);
    }
    if let Ok(pw) = std::env::var(env_var) {
        if !pw.is_empty() {
            return Ok(pw);
        }
    }
    match prompt(ctx, label, true)? {
        Some(pw) if !pw.is_empty() => Ok(pw),
        _ => Err(crate::output::error(
            command,
            ctx,
            "PASSWORD_REQUIRED",
            &format!("no password: pass --password-file, set {env_var}, or run interactively"),
        )),
    }
}

/// Resolve an OPTIONAL password (owner password, P12 convenience): file,
/// then env, then — on a TTY only — prompt. Piped stdin with no password
/// means "none", not an error.
pub(crate) fn optional_password(
    ctx: &Ctx,
    command: &str,
    file: Option<&Path>,
    env_var: &str,
    label: &str,
) -> Result<Option<String>, u8> {
    if let Some(path) = file {
        let pw = read_password_file(path, 1)
            .map_err(|(code, msg)| crate::output::error(command, ctx, &code, &msg))?;
        return Ok(if pw.is_empty() { None } else { Some(pw) });
    }
    if let Ok(pw) = std::env::var(env_var) {
        if !pw.is_empty() {
            return Ok(Some(pw));
        }
    }
    if !io::stdin().is_terminal() {
        return Ok(None);
    }
    match prompt(ctx, label, false)? {
        Some(pw) if !pw.is_empty() => Ok(Some(pw)),
        _ => Ok(None),
    }
}

/// Prompt on stderr, read one line from stdin. On a Unix TTY echo is off and
/// the entry is confirmed; piped input reads a single line. `None` is EOF.
fn prompt(ctx: &Ctx, label: &str, confirm: bool) -> Result<Option<String>, u8> {
    let tty = io::stdin().is_terminal();
    let first = prompt_line(ctx, label, tty)?;
    let Some(first) = first else {
        return Ok(None);
    };
    if confirm && tty {
        let second = prompt_line(ctx, &format!("{label} (repeat)"), tty)?;
        if second.as_deref() != Some(first.as_str()) {
            return Err(crate::output::error(
                "password",
                ctx,
                "PASSWORD_MISMATCH",
                "the two passwords differ; nothing was done",
            ));
        }
    }
    Ok(Some(first))
}

fn prompt_line(ctx: &Ctx, label: &str, tty: bool) -> Result<Option<String>, u8> {
    eprint!("{label}: ");
    let _ = io::stderr().flush();
    let _guard = tty.then(EchoOff::new);
    if tty && _guard.as_ref().is_some_and(|g| !g.off) && !ctx.quiet {
        eprintln!("(terminal echo is on; prefer --password-file)");
    }
    let mut line = String::new();
    let stdin = io::stdin();
    match stdin.lock().read_line(&mut line) {
        Ok(0) => {
            eprintln!();
            Ok(None)
        }
        Ok(_) => {
            if tty {
                eprintln!();
            }
            let text = line.strip_suffix('\n').unwrap_or(&line);
            let text = text.strip_suffix('\r').unwrap_or(text);
            Ok(Some(text.to_string()))
        }
        Err(e) => Err(crate::output::error(
            "password",
            ctx,
            "UNREADABLE_INPUT",
            &format!("could not read from stdin: {e}"),
        )),
    }
}

/// RAII terminal-echo switch (Unix only). The old flags are restored on drop
/// so a Ctrl-C or early return never leaves the terminal muted.
struct EchoOff {
    #[cfg(unix)]
    fd: i32,
    #[cfg(unix)]
    previous: libc::termios,
    off: bool,
}

impl EchoOff {
    fn new() -> Self {
        #[cfg(unix)]
        {
            // SAFETY: tcgetattr/tcsetattr on stdin with stack-local storage.
            // The fd is valid (stdin exists in every CLI invocation) and the
            // termios struct is written only through these calls.
            unsafe {
                let fd = libc::STDIN_FILENO;
                let mut previous: libc::termios = std::mem::zeroed();
                if libc::tcgetattr(fd, &mut previous) != 0 {
                    return Self {
                        fd,
                        previous,
                        off: false,
                    };
                }
                let mut muted = previous;
                muted.c_lflag &= !libc::ECHO;
                if libc::tcsetattr(fd, libc::TCSANOW, &muted) != 0 {
                    return Self {
                        fd,
                        previous,
                        off: false,
                    };
                }
                Self {
                    fd,
                    previous,
                    off: true,
                }
            }
        }
        #[cfg(not(unix))]
        {
            Self { off: false }
        }
    }
}

#[cfg(unix)]
impl Drop for EchoOff {
    fn drop(&mut self) {
        if self.off {
            // SAFETY: same fd and struct as construction; best-effort restore.
            unsafe {
                libc::tcsetattr(self.fd, libc::TCSANOW, &self.previous);
            }
        }
    }
}

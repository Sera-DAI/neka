//! Secret input and clipboard (platform dependent).

use std::io::{BufRead, IsTerminal, Write};

use zeroize::Zeroizing;

use crate::ui;

/// Reads a secret without echo. With no terminal (script or test), reads a line from
/// standard input. Never comes as an argument (project rule).
pub fn secret(prompt: &str) -> Result<Zeroizing<String>, String> {
    if std::io::stdin().is_terminal() {
        read_hidden(prompt)
    } else {
        eprint!("{prompt}");
        line()
    }
}

/// Reads from the terminal without echo. With decoration, the prompt is written by `ui` (in color);
/// without decoration, lets `rpassword` write it, as before.
fn read_hidden(prompt: &str) -> Result<Zeroizing<String>, String> {
    let r = if ui::decorated(ui::Flow::Error) {
        ui::prompt(prompt);
        rpassword::read_password()
    } else {
        rpassword::prompt_password(prompt)
    };
    r.map(Zeroizing::new)
        .map_err(|_| "could not read from the terminal".to_owned())
}

/// Reads a plain line (confirmations).
pub fn line() -> Result<Zeroizing<String>, String> {
    let mut s = Zeroizing::new(String::new());
    std::io::stdin()
        .lock()
        .read_line(&mut s)
        .map_err(|_| "could not read the input".to_owned())?;
    while s.ends_with('\n') || s.ends_with('\r') {
        s.pop();
    }
    Ok(s)
}

pub fn confirm(question: &str) -> Result<bool, String> {
    // On the terminal, reads through the same path as the password (rpassword): on Windows,
    // reading the line from standard input after the password prompt could
    // wait forever. The answer is echoed afterwards.
    let r = if std::io::stdin().is_terminal() {
        let r = read_hidden(&format!("{question} [y/N] "))?;
        eprintln!("{}", r.trim());
        r
    } else {
        eprint!("{question} [y/N] ");
        let _ = std::io::stderr().flush();
        line()?
    };
    Ok(matches!(
        r.trim().to_lowercase().as_str(),
        "y" | "yes" | "s" | "sim"
    ))
}

/// Asks for the new password twice. Refuses fewer than 8 characters and warns
/// below 12.
pub fn new_password(label: &str) -> Result<Zeroizing<String>, String> {
    let a = secret(&format!("{label}: "))?;
    if a.chars().count() < 8 {
        return Err("the password must have at least 8 characters".into());
    }
    let b = secret("Repeat: ")?;
    if a != b {
        return Err("the passwords do not match".into());
    }
    if a.chars().count() < 12 {
        ui::warning("passwords shorter than 12 characters are weak for a master password.");
    }
    Ok(a)
}

/// Puts text on the clipboard. On Windows, asks to stay out of the
/// history (Win+V) and the cloud.
pub fn put(cb: &mut arboard::Clipboard, text: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        use arboard::SetExtWindows;
        cb.set()
            .exclude_from_history()
            .exclude_from_cloud()
            .text(text)
            .map_err(|_| "could not copy".to_owned())
    }
    #[cfg(not(windows))]
    cb.set_text(text).map_err(|_| "could not copy".to_owned())
}

/// Copies to the clipboard and clears after `seconds`, if the
/// content is still the same. On Windows, asks to stay out of the history
/// (Win+V) and the cloud.
pub fn copy(text: &str, seconds: u64) -> Result<(), String> {
    let mut cb = arboard::Clipboard::new().map_err(|_| "clipboard unavailable".to_owned())?;
    put(&mut cb, text)?;
    if seconds == 0 {
        return Ok(());
    }
    ui::success(
        ui::Flow::Error,
        format!("Copied. The clipboard will be cleared in {seconds} s (Ctrl+C to leave earlier, without clearing)."),
    );
    std::thread::sleep(std::time::Duration::from_secs(seconds));
    if cb.get_text().ok().as_deref() == Some(text) {
        let _ = cb.clear();
        ui::hint(ui::Flow::Error, "Clipboard cleared.");
    }
    Ok(())
}

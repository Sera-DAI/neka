//! Entrada de segredos e área de transferência (dependem da plataforma).

use std::io::{BufRead, IsTerminal, Write};

use zeroize::Zeroizing;

use crate::ui;

/// Lê um segredo sem eco. Sem terminal (script ou teste), lê uma linha da
/// entrada padrão. Nunca vem por argumento (regra do projeto).
pub fn segredo(prompt: &str) -> Result<Zeroizing<String>, String> {
    if std::io::stdin().is_terminal() {
        ler_sem_eco(prompt)
    } else {
        eprint!("{prompt}");
        linha()
    }
}

/// Lê do terminal sem eco. Com enfeite, o prompt é escrito por `ui` (com cor);
/// sem enfeite, deixa o `rpassword` escrevê-lo, como antes.
fn ler_sem_eco(prompt: &str) -> Result<Zeroizing<String>, String> {
    let r = if ui::enfeitado(ui::Fluxo::Erro) {
        ui::prompt(prompt);
        rpassword::read_password()
    } else {
        rpassword::prompt_password(prompt)
    };
    r.map(Zeroizing::new)
        .map_err(|_| "could not read from the terminal".to_owned())
}

/// Lê uma linha comum (confirmações).
pub fn linha() -> Result<Zeroizing<String>, String> {
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

pub fn confirmar(pergunta: &str) -> Result<bool, String> {
    // No terminal, lê pelo mesmo caminho da senha (rpassword): no Windows,
    // ler a linha pela entrada padrão depois do prompt de senha podia
    // ficar esperando para sempre. A resposta é ecoada em seguida.
    let r = if std::io::stdin().is_terminal() {
        let r = ler_sem_eco(&format!("{pergunta} [y/N] "))?;
        eprintln!("{}", r.trim());
        r
    } else {
        eprint!("{pergunta} [y/N] ");
        let _ = std::io::stderr().flush();
        linha()?
    };
    Ok(matches!(
        r.trim().to_lowercase().as_str(),
        "y" | "yes" | "s" | "sim"
    ))
}

/// Pede a senha nova duas vezes. Recusa menos de 8 caracteres e avisa
/// abaixo de 12.
pub fn senha_nova(rotulo: &str) -> Result<Zeroizing<String>, String> {
    let a = segredo(&format!("{rotulo}: "))?;
    if a.chars().count() < 8 {
        return Err("the password must have at least 8 characters".into());
    }
    let b = segredo("Repeat: ")?;
    if a != b {
        return Err("the passwords do not match".into());
    }
    if a.chars().count() < 12 {
        ui::aviso("passwords shorter than 12 characters are weak for a master password.");
    }
    Ok(a)
}

/// Coloca texto na área de transferência. No Windows, pede para ficar fora do
/// histórico (Win+V) e da nuvem.
pub fn colocar(cb: &mut arboard::Clipboard, texto: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        use arboard::SetExtWindows;
        cb.set()
            .exclude_from_history()
            .exclude_from_cloud()
            .text(texto)
            .map_err(|_| "could not copy".to_owned())
    }
    #[cfg(not(windows))]
    cb.set_text(texto).map_err(|_| "could not copy".to_owned())
}

/// Copia para a área de transferência e limpa depois de `segundos`, se o
/// conteúdo ainda for o mesmo. No Windows, pede para ficar fora do histórico
/// (Win+V) e da nuvem.
pub fn copiar(texto: &str, segundos: u64) -> Result<(), String> {
    let mut cb = arboard::Clipboard::new().map_err(|_| "clipboard unavailable".to_owned())?;
    colocar(&mut cb, texto)?;
    if segundos == 0 {
        return Ok(());
    }
    ui::sucesso(
        ui::Fluxo::Erro,
        format!("Copied. The clipboard will be cleared in {segundos} s (Ctrl+C to leave earlier, without clearing)."),
    );
    std::thread::sleep(std::time::Duration::from_secs(segundos));
    if cb.get_text().ok().as_deref() == Some(texto) {
        let _ = cb.clear();
        ui::dica(ui::Fluxo::Erro, "Clipboard cleared.");
    }
    Ok(())
}

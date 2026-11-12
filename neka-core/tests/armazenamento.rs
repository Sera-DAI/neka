//! Critérios de aceite do armazenamento (etapa 4), conforme a lista
//! "Testes obrigatórios" de a especificação do formato.

use std::io;
use std::path::PathBuf;

use neka_core::crypto::{Chave, ErroCripto, ParametrosKdf};
use neka_core::model::{Corpo, Entrada, Uuid};
use neka_core::store::*;

const RAPIDO: ParametrosKdf = ParametrosKdf {
    mem_kib: ParametrosKdf::MEM_MIN_KIB,
    iters: 1,
    lanes: 1,
};
const SENHA: &[u8] = b"senha mestra ficticia";

fn corpo() -> Corpo {
    let mut c = Corpo::default();
    let mut e = Entrada::nova(
        Uuid::from_u128(1),
        Uuid::from_u128(9),
        "exemplo".into(),
        1_700_000_000_000,
        1,
    );
    e.password = "senha-ficticia-da-entrada".into();
    c.entries.push(e);
    c.device_clock = 1;
    c
}

fn novo() -> (CofreAberto, Chave, Vec<u8>) {
    let (mut c, rec) = CofreAberto::criar(SENHA, RAPIDO, corpo()).unwrap();
    let b = c.serializar().unwrap();
    (c, rec, b)
}

/// Deslocamentos no arquivo v1.
const OFF_VERSAO: usize = 4;
const OFF_FLAGS: usize = 6;
const OFF_EPOCA: usize = 24;
const OFF_MEM: usize = 29;
const OFF_CONTAGEM: usize = 57;
const OFF_SLOTS: usize = OFF_CONTAGEM + 1;
const TAM_SLOT: usize = 1 + 24 + 48;

#[test]
fn abre_com_senha_e_recupera_o_corpo() {
    let (_, _, b) = novo();
    assert_eq!(&b[..4], b"NEKA");
    let c = CofreAberto::abrir_com_senha(&b, SENHA).unwrap();
    assert_eq!(c.corpo(), &corpo());
}

#[test]
fn senha_errada_falha() {
    let (_, _, b) = novo();
    let e = CofreAberto::abrir_com_senha(&b, b"senha errada").unwrap_err();
    assert_eq!(e, ErroArmazenamento::Cripto(ErroCripto::FalhaAutenticacao));
}

#[test]
fn abre_com_recuperacao() {
    let (_, rec, b) = novo();
    let c = CofreAberto::abrir_com_recuperacao(&b, &rec).unwrap();
    assert_eq!(c.corpo(), &corpo());
    assert!(CofreAberto::abrir_com_recuperacao(&b, &Chave::aleatoria().unwrap()).is_err());
}

#[test]
fn qualquer_byte_alterado_faz_a_abertura_falhar() {
    let (_, rec, b) = novo();
    for i in 0..b.len() {
        let mut m = b.clone();
        m[i] ^= 0x01;
        let por_senha = CofreAberto::abrir_com_senha(&m, SENHA);
        let por_rec = CofreAberto::abrir_com_recuperacao(&m, &rec);
        // Um byte do compartimento de um tipo só pode impedir aquele tipo;
        // em nenhum caso o arquivo alterado abre pelos dois caminhos.
        assert!(
            por_senha.is_err() || por_rec.is_err(),
            "byte {i} alterado e o arquivo abriu pelos dois caminhos"
        );
        // Parâmetros do KDF e sal só servem ao compartimento de senha (D14):
        // alterá-los só pode impedir aquele caminho. Fora disso, nada abre.
        let do_slot_de_senha = (OFF_MEM..OFF_CONTAGEM).contains(&i);
        let em_slot = do_slot_de_senha || (OFF_SLOTS..OFF_SLOTS + 2 * TAM_SLOT).contains(&i);
        if do_slot_de_senha {
            assert!(por_senha.is_err(), "byte {i}");
        }
        if !em_slot {
            assert!(por_senha.is_err() && por_rec.is_err(), "byte {i}");
        }
    }
}

#[test]
fn compartimento_corrompido_nao_impede_abrir_pelo_outro() {
    let (_, rec, b) = novo();
    // estraga a chave embrulhada do compartimento de senha (1º slot)
    let mut m = b.clone();
    m[OFF_SLOTS + 1 + 24 + 5] ^= 0xFF;
    assert!(CofreAberto::abrir_com_senha(&m, SENHA).is_err());
    assert_eq!(
        CofreAberto::abrir_com_recuperacao(&m, &rec)
            .unwrap()
            .corpo(),
        &corpo()
    );
    // estraga o de recuperação (2º slot)
    let mut m = b.clone();
    m[OFF_SLOTS + TAM_SLOT + 1 + 24 + 5] ^= 0xFF;
    assert!(CofreAberto::abrir_com_recuperacao(&m, &rec).is_err());
    assert_eq!(
        CofreAberto::abrir_com_senha(&m, SENHA).unwrap().corpo(),
        &corpo()
    );
}

#[test]
fn trocar_senha_gira_a_chave_e_revoga_a_senha_e_a_recuperacao_antigas() {
    let (mut c, rec_antiga, antes) = novo();
    assert_eq!(c.epoca_da_chave(), 0);
    let rec_nova = c.trocar_senha(b"nova senha ficticia", RAPIDO).unwrap();
    assert_eq!(c.epoca_da_chave(), 1);
    let depois = c.serializar().unwrap();

    // a época vai no cabeçalho e o corpo cifrado muda de verdade
    assert_eq!(depois[OFF_EPOCA..OFF_EPOCA + 4], 1u32.to_le_bytes());
    let ini = OFF_SLOTS + 2 * TAM_SLOT;
    assert_ne!(antes[ini..], depois[ini..], "o corpo deveria ser recifrado");

    // senha antiga e recuperação antiga não abrem o arquivo novo
    assert!(CofreAberto::abrir_com_senha(&depois, SENHA).is_err());
    assert!(CofreAberto::abrir_com_recuperacao(&depois, &rec_antiga).is_err());
    // credenciais novas abrem, com o mesmo conteúdo
    let c2 = CofreAberto::abrir_com_senha(&depois, b"nova senha ficticia").unwrap();
    assert_eq!(c2.corpo(), &corpo());
    assert_eq!(c2.epoca_da_chave(), 1);
    assert!(CofreAberto::abrir_com_recuperacao(&depois, &rec_nova).is_ok());

    // a cópia antiga continua abrindo com as credenciais antigas (limite declarado)
    assert!(CofreAberto::abrir_com_senha(&antes, SENHA).is_ok());
    assert!(CofreAberto::abrir_com_recuperacao(&antes, &rec_antiga).is_ok());
}

#[test]
fn epoca_adulterada_no_cabecalho_faz_a_abertura_falhar() {
    let (mut c, rec, _) = novo();
    c.trocar_senha(b"nova senha ficticia", RAPIDO).unwrap();
    let b = c.serializar().unwrap();
    let mut m = b.clone();
    m[OFF_EPOCA..OFF_EPOCA + 4].copy_from_slice(&0u32.to_le_bytes());
    assert!(CofreAberto::abrir_com_senha(&m, b"nova senha ficticia").is_err());
    let _ = rec;
}

#[test]
fn corpo_alterado_ganha_nonce_novo() {
    let (mut c, _, antes) = novo();
    c.corpo_mut().device_clock = 2;
    let depois = c.serializar().unwrap();
    let ini = OFF_SLOTS + 2 * TAM_SLOT;
    assert_ne!(antes[ini..ini + 24], depois[ini..ini + 24]);
    assert_eq!(
        CofreAberto::abrir_com_senha(&depois, SENHA)
            .unwrap()
            .corpo()
            .device_clock,
        2
    );
}

#[test]
fn recusa_versao_mais_nova_flags_e_lixo() {
    let (_, _, b) = novo();
    let mut m = b.clone();
    m[OFF_VERSAO..OFF_VERSAO + 2].copy_from_slice(&2u16.to_le_bytes());
    assert_eq!(
        CofreAberto::abrir_com_senha(&m, SENHA).unwrap_err(),
        ErroArmazenamento::VersaoMaisNova
    );
    let mut m = b.clone();
    m[OFF_FLAGS] = 1;
    assert_eq!(
        CofreAberto::abrir_com_senha(&m, SENHA).unwrap_err(),
        ErroArmazenamento::FlagsDesconhecidas
    );
    for lixo in [&b""[..], b"NEKA", b"KEEPASS!", &b[..b.len() - 1]] {
        assert_eq!(
            CofreAberto::abrir_com_senha(lixo, SENHA).unwrap_err(),
            ErroArmazenamento::FormatoInvalido
        );
    }
    let mut m = b.clone();
    m.push(0);
    assert_eq!(
        CofreAberto::abrir_com_senha(&m, SENHA).unwrap_err(),
        ErroArmazenamento::FormatoInvalido
    );
}

#[test]
fn recusa_parametros_do_kdf_fora_dos_limites_antes_de_derivar() {
    let (_, _, b) = novo();
    for mem in [1u32, ParametrosKdf::MEM_MAX_KIB + 1, u32::MAX] {
        let mut m = b.clone();
        m[OFF_MEM..OFF_MEM + 4].copy_from_slice(&mem.to_le_bytes());
        assert_eq!(
            CofreAberto::abrir_com_senha(&m, SENHA).unwrap_err(),
            ErroArmazenamento::Cripto(ErroCripto::ParametrosForaDosLimites)
        );
    }
}

#[test]
fn erros_nao_vazam_segredos() {
    let (c, _, b) = novo();
    let s = format!(
        "{c:?} {}",
        CofreAberto::abrir_com_senha(&b, b"x").unwrap_err()
    );
    assert!(!s.contains("senha-ficticia-da-entrada") && !s.contains("senha mestra"));
}

// ---- gravação atômica ----

fn pasta() -> PathBuf {
    let d = std::env::temp_dir().join(format!("neka-teste-{}", Uuid::from_bytes(rand16())));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Arquivos `.tmp` esquecidos na pasta.
fn sobras(d: &std::path::Path) -> usize {
    std::fs::read_dir(d)
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp")
        })
        .count()
}

fn rand16() -> [u8; 16] {
    let mut b = [0u8; 16];
    neka_core::crypto::aleatorio(&mut b).unwrap();
    b
}

#[test]
fn gravacao_atomica_com_falha_em_cada_passo_preserva_o_original() {
    let (mut c, _, original) = novo();
    c.corpo_mut().device_clock = 7;
    let novo_bytes = c.serializar().unwrap();
    let passos = [
        Passo::CriarTemporario,
        Passo::Escrever,
        Passo::Sincronizar,
        Passo::Renomear,
    ];
    for p in passos {
        let d = pasta();
        let alvo = d.join("cofre.neka");
        gravar_atomico(&alvo, &original).unwrap();
        let r = gravar_atomico_com_falha(&alvo, &novo_bytes, |q| {
            if q == p {
                Err(io::Error::other("falha injetada"))
            } else {
                Ok(())
            }
        });
        assert!(r.is_err(), "{p:?}");
        assert_eq!(
            ler_arquivo(&alvo).unwrap(),
            original,
            "original mudou com falha em {p:?}"
        );
        assert_eq!(sobras(&d), 0, "temporário ficou para trás em {p:?}");
        assert!(CofreAberto::abrir_com_senha(&ler_arquivo(&alvo).unwrap(), SENHA).is_ok());
        std::fs::remove_dir_all(&d).unwrap();
    }
    // Falha ao sincronizar a pasta, depois da renomeação: o arquivo novo já
    // está inteiro no lugar, então a gravação conta como feita.
    let d = pasta();
    let alvo = d.join("cofre.neka");
    gravar_atomico(&alvo, &original).unwrap();
    let r = gravar_atomico_com_falha(&alvo, &novo_bytes, |q| {
        if q == Passo::SincronizarPasta {
            Err(io::Error::other("falha injetada"))
        } else {
            Ok(())
        }
    });
    assert!(r.is_ok());
    assert_eq!(ler_arquivo(&alvo).unwrap(), novo_bytes);
    assert_eq!(sobras(&d), 0);
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn gravacao_sem_falha_substitui_e_backup_copia() {
    let d = pasta();
    let alvo = d.join("cofre.neka");
    let (_, _, b) = novo();
    gravar_atomico(&alvo, b"versao antiga").unwrap();
    let bak = backup(&alvo).unwrap();
    assert_eq!(bak, d.join("cofre.neka.bak"));
    gravar_atomico(&alvo, &b).unwrap();
    assert_eq!(ler_arquivo(&alvo).unwrap(), b);
    assert_eq!(ler_arquivo(&bak).unwrap(), b"versao antiga");
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn cofre_grande_demais_nao_e_gravado() {
    // Centenas de entradas no tamanho máximo passam de 64 MiB depois de cifradas.
    let mut c = corpo();
    for n in 10..230u128 {
        let mut e = Entrada::nova(
            Uuid::from_u128(n),
            Uuid::from_u128(9),
            format!("e{n}"),
            1,
            1,
        );
        e.notes = "x".repeat(64 * 1024);
        for i in 0..64 {
            e.extra.insert(format!("k{i}"), "y".repeat(4096));
        }
        c.entries.push(e);
    }
    let (mut cofre, _) = CofreAberto::criar(SENHA, RAPIDO, c).unwrap();
    assert_eq!(
        cofre.serializar().unwrap_err(),
        ErroArmazenamento::ArquivoGrandeDemais
    );
}

#[cfg(unix)]
#[test]
fn temporario_nasce_restrito_e_nao_segue_link_plantado() {
    use std::os::unix::fs::PermissionsExt;
    let d = pasta();
    let alvo = d.join("cofre.neka");
    gravar_atomico(&alvo, b"conteudo").unwrap();
    assert_eq!(
        std::fs::metadata(&alvo).unwrap().permissions().mode() & 0o777,
        0o600
    );
    // link simbólico plantado com o nome antigo do temporário não é seguido
    let vitima = d.join("vitima.txt");
    std::fs::write(&vitima, b"intacto").unwrap();
    std::os::unix::fs::symlink(&vitima, d.join("cofre.neka.tmp")).unwrap();
    gravar_atomico(&alvo, b"novo").unwrap();
    assert_eq!(std::fs::read(&vitima).unwrap(), b"intacto");
    std::fs::remove_dir_all(&d).unwrap();
}

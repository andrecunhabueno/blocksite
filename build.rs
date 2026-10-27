use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let icon_dir = manifest_dir.join("assets");
    let icon = icon_dir.join("blocksite.ico");
    let resource = icon_dir.join("blocksite.rc");

    println!("cargo:rerun-if-changed={}", icon.display());
    println!("cargo:rerun-if-changed={}", resource.display());

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let Ok(out_dir) = env::var("OUT_DIR") else {
        return;
    };
    let object = PathBuf::from(out_dir).join("blocksite_icon.o");
    match Command::new("windres")
        .current_dir(&icon_dir)
        .args(["blocksite.rc", "-O", "coff", "-o"])
        .arg(&object)
        .status()
    {
        Ok(status) if status.success() => {
            println!("cargo:rustc-link-arg={}", object.display());
        }
        Ok(status) => {
            println!(
                "cargo:warning=windres não conseguiu compilar o ícone (código {status}); o ícone da janela ainda será carregado em tempo de execução."
            );
        }
        Err(error) => {
            println!(
                "cargo:warning=windres não encontrado ({error}); o ícone do executável não será incorporado."
            );
        }
    }
}

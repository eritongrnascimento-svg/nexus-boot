fn main() {
    println!("cargo:rerun-if-changed=nexus-boot.rc");
    println!("cargo:rerun-if-changed=nexus-boot.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows")
        || std::env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") { return; }
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
    let res = output.join("nexus-boot.res");
    let status = std::process::Command::new("rc.exe")
        .arg("/nologo").arg("/fo").arg(&res).arg("nexus-boot.rc")
        .status().expect("Execute a compilação no prompt x64 do Visual Studio com o Windows SDK.");
    assert!(status.success(), "Não foi possível compilar o ícone do Nexus Boot.");
    println!("cargo:rustc-link-arg-bin=nexus-manager={}", res.display());
}

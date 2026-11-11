fn main() {
    println!("cargo:rerun-if-changed=assets/editortool.ico");
    println!("cargo:rerun-if-changed=assets/windows.rc");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("assets/windows.rc", embed_resource::NONE)
            .manifest_required()
            .expect("Windows-Icon konnte nicht eingebettet werden");
    }
}

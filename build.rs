fn main() {
    println!("cargo:rerun-if-changed=assets/app.rc");
    println!("cargo:rerun-if-changed=assets/screensleep.ico");
    println!("cargo:rerun-if-changed=assets/screensleep-light.ico");
    println!("cargo:rerun-if-changed=assets/app.manifest");
    // Embeds the icon into the .exe. If no resource compiler is installed the
    // build still succeeds and the app falls back to a stock icon.
    let _ = embed_resource::compile("assets/app.rc", embed_resource::NONE);
}

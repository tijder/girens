//! libmpv2-sys links `-lmpv` without telling the linker where to look. In the
//! Flatpak, libmpv lives in /app/lib, so ask pkg-config for the search path.

fn main() {
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_PATH");
    match pkg_config::Config::new()
        .atleast_version("2.0")
        .probe("mpv")
    {
        Ok(_) => {}
        Err(err) => println!("cargo:warning=libmpv was not found through pkg-config: {err}"),
    }
}

fn main() {
    println!("cargo:rerun-if-changed=assets/rc505-rs-icon-v1.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winres::WindowsResource::new()
            .set_icon("assets/rc505-rs-icon-v1.ico")
            .set("ProductName", "RC505 RS")
            .set("FileDescription", "RC505 RS loop station")
            .set("CompanyName", "Yishanka")
            .compile()
            .expect("Windows icon/version resource compilation failed");
    }
}

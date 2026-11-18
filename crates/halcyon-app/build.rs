fn main() {
    // Traductions : catalogues de locales/ fusionnés pour ce binaire, intégrés à l'exécutable.
    let locales = halcyon_i18n::build::assemble(
        std::path::Path::new("../../locales"),
        &std::env::var("CARGO_PKG_NAME").unwrap(),
        halcyon_i18n::build::Domains::AllExcept(&["installer"]),
    )
    .expect("assemblage des traductions");
    let config = slint_build::CompilerConfiguration::new()
        .with_style("fluent-dark".into())
        .with_bundled_translations(locales)
        .with_default_translation_context(slint_build::DefaultTranslationContext::None);
    slint_build::compile_with_config("ui/app.slint", config).expect("compilation de l'interface Slint");

    // Windows : icône et informations de version intégrées à l'exécutable.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../packaging/windows/halcyon.ico")
            .set("ProductName", "Halcyon")
            .set("FileDescription", "Halcyon — launcher universel de jeux vidéo")
            .set("LegalCopyright", "MIT");
        if let Err(e) = res.compile() {
            println!("cargo:warning=icône Windows non intégrée : {e}");
        }
    }
    println!("cargo:rerun-if-changed=../../packaging/windows/halcyon.ico");
}

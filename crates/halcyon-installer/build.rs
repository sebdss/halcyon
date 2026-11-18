fn main() {
    // Traductions : catalogues de locales/ fusionnés pour ce binaire, intégrés à l'exécutable.
    let locales = halcyon_i18n::build::assemble(
        std::path::Path::new("../../locales"),
        &std::env::var("CARGO_PKG_NAME").unwrap(),
        halcyon_i18n::build::Domains::Only(&["installer", "formats"]),
    )
    .expect("assemblage des traductions");
    let config = slint_build::CompilerConfiguration::new()
        .with_style("fluent-dark".into())
        .with_bundled_translations(locales)
        .with_default_translation_context(slint_build::DefaultTranslationContext::None);
    slint_build::compile_with_config("ui/setup.slint", config).expect("compilation de l'interface de l'installateur");

    // Windows : icône, informations de version et manifeste « asInvoker » (l'installation
    // par utilisateur ne demande pas de droits d'administrateur).
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../packaging/windows/halcyon.ico")
            .set("ProductName", "Halcyon")
            .set("FileDescription", "Installation de Halcyon")
            .set("LegalCopyright", "MIT")
            .set_manifest(
                r#"<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
<trustInfo xmlns="urn:schemas-microsoft-com:asm.v3"><security><requestedPrivileges>
<requestedExecutionLevel level="asInvoker" uiAccess="false"/>
</requestedPrivileges></security></trustInfo>
<application xmlns="urn:schemas-microsoft-com:asm.v3"><windowsSettings>
<dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>
<dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
</windowsSettings></application>
</assembly>"#,
            );
        if let Err(e) = res.compile() {
            println!("cargo:warning=ressources Windows non intégrées : {e}");
        }
    }
    println!("cargo:rerun-if-changed=../../packaging/windows/halcyon.ico");
}

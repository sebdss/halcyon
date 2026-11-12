//! Minecraft Java Edition dans Halcyon.
//!
//! - `meta` : versions officielles, fichiers de version, règles, héritage des loaders ;
//! - `install` : téléchargements vérifiés, bibliothèques, natives, assets, vérification ;
//! - `java` : détection et runtimes officiels de Mojang ;
//! - `launch` : ligne de commande et démarrage ;
//! - `loaders` : Fabric, Quilt, Forge, NeoForge ;
//! - `auth` : compte Microsoft, licence, sessions chiffrées ;
//! - `mods`, `modpack` : Modrinth, CurseForge, `.mrpack` ;
//! - `profiles` : installations séparées (mods, mondes, réglages) ;
//! - `official` : jouer par le launcher officiel (profils ajoutés au launcher de Mojang).
//!
//! Les fichiers sont rangés dans un dossier propre à Halcyon : le `.minecraft` du launcher
//! officiel n'est jamais modifié.

pub mod auth;
pub mod http;
pub mod install;
pub mod java;
pub mod launch;
pub mod loaders;
pub mod meta;
pub mod modpack;
pub mod mods;
pub mod official;
pub mod profiles;

pub use install::{Event, Layout};
pub use loaders::Loader;

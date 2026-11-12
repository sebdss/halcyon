//! Empreintes RetroAchievements des ROM « cartouche ».
//!
//! RetroAchievements identifie un jeu par le MD5 de sa ROM, après normalisation
//! propre à chaque console (en-têtes retirés, ordre des octets N64…). Seules les
//! méthodes simples et documentées sont implémentées ; les jeux sur disque
//! (PS1, PS2, GameCube…) et les archives `.zip` ne sont pas hachés : l'identifiant
//! RetroAchievements peut alors être saisi à la main dans la fiche du jeu.

use md5::{Digest, Md5};
use std::path::Path;

/// Taille maximale hachée (au-delà : ce n'est pas une ROM cartouche).
const MAX_SIZE: u64 = 64 * 1024 * 1024;

/// Calcule l'empreinte. `Ok(None)` : méthode non prise en charge pour ce fichier.
pub fn hash_rom(path: &Path, method: &str) -> std::io::Result<Option<String>> {
    if method == "none" || path.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip") || e.eq_ignore_ascii_case("7z")) {
        return Ok(None);
    }
    if std::fs::metadata(path)?.len() > MAX_SIZE {
        return Ok(None);
    }
    let data = std::fs::read(path)?;
    Ok(hash_bytes(&data, method))
}

pub fn hash_bytes(data: &[u8], method: &str) -> Option<String> {
    let body: std::borrow::Cow<[u8]> = match method {
        "plain" => data.into(),
        "nes" if data.starts_with(b"NES\x1a") || data.starts_with(b"FDS\x1a") => data[16.min(data.len())..].into(),
        "nes" => data.into(),
        "snes" if data.len() % 8192 == 512 => data[512..].into(),
        "snes" => data.into(),
        "pce" if data.len() % 131_072 == 512 => data[512..].into(),
        "pce" => data.into(),
        "lynx" if data.starts_with(b"LYNX\0") => data[64.min(data.len())..].into(),
        "lynx" => data.into(),
        "7800" if data.len() > 128 && &data[1..10] == b"ATARI7800" => data[128..].into(),
        "7800" => data.into(),
        "n64" => n64_to_big_endian(data)?.into(),
        _ => return None,
    };
    Some(format!("{:x}", Md5::digest(&body)))
}

/// Convertit une ROM N64 (.v64 permutée, .n64 petit-boutiste) au format .z64.
fn n64_to_big_endian(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 4 {
        return None;
    }
    let mut out = data.to_vec();
    match &data[..4] {
        [0x80, 0x37, 0x12, 0x40] => {}
        [0x37, 0x80, 0x40, 0x12] => out.chunks_exact_mut(2).for_each(|c| c.swap(0, 1)),
        [0x40, 0x12, 0x37, 0x80] => out.chunks_exact_mut(4).for_each(|c| c.reverse()),
        _ => return None,
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn md5(b: &[u8]) -> String {
        format!("{:x}", Md5::digest(b))
    }

    #[test]
    fn normalizes_headers_and_byte_order() {
        assert_eq!(hash_bytes(b"", "plain").unwrap(), "d41d8cd98f00b204e9800998ecf8427e");
        let body = vec![7u8; 32];
        let mut nes = b"NES\x1a".to_vec();
        nes.extend([0u8; 12]);
        nes.extend(&body);
        assert_eq!(hash_bytes(&nes, "nes").unwrap(), md5(&body));

        let mut snes = vec![0u8; 512];
        snes.extend(vec![1u8; 8192]);
        assert_eq!(hash_bytes(&snes, "snes").unwrap(), md5(&vec![1u8; 8192]));

        let z64 = [0x80, 0x37, 0x12, 0x40, 1, 2, 3, 4];
        let v64 = [0x37, 0x80, 0x40, 0x12, 2, 1, 4, 3];
        let n64 = [0x40, 0x12, 0x37, 0x80, 4, 3, 2, 1];
        assert_eq!(hash_bytes(&v64, "n64"), hash_bytes(&z64, "n64"));
        assert_eq!(hash_bytes(&n64, "n64"), hash_bytes(&z64, "n64"));
        assert_eq!(hash_bytes(&[1, 2, 3, 4], "n64"), None);
        assert_eq!(hash_bytes(&body, "none"), None);
    }
}
